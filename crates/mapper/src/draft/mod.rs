//! Running `srs draft`: load a draft and its base map, lay the draft over
//! the base, and check it, write its page, or preview it live. The work is
//! done by map-persist, map-draft, map-web and map-preview; this module
//! composes them. Files, the preview and the watcher are injected.

use map_draft::{BaseMismatch, DraftProblem};
use map_persist::{LoadError, MapDocument};
use project_map::{Draft, DraftBase, MapPart, MapProblem, ProjectMap};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// What `srs draft` should do with a draft file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftCommand {
    /// Report the draft's problems; fail if there are any.
    Check { draft: PathBuf },
    /// Write the draft's page to `out`; fail if the draft has problems.
    Render { draft: PathBuf, out: PathBuf },
    /// Serve a live preview at `listen`, rebuilt whenever the draft or its
    /// base changes, until stopped.
    Preview {
        draft: PathBuf,
        listen: std::net::SocketAddr,
    },
}

/// The files `srs draft` reads and writes, and what it tells the person
/// running it.
pub trait DraftFiles {
    fn read(&self, path: &Path) -> std::io::Result<String>;
    fn write(&mut self, path: &Path, text: &str) -> std::io::Result<()>;
    fn note(&mut self, message: &str);
}

/// Where a preview shows each build.
pub trait PreviewSink {
    fn show_page(&self, page: String);
    fn show_problems(&self, problems: Vec<String>);
}

/// Watches a set of files.
pub trait WatchFiles {
    /// Watches `paths` from now on: what they hold now is the baseline, and
    /// only a later change counts.
    fn watch(&mut self, paths: &[PathBuf]) -> Result<(), String>;
    /// Waits until any watched file changes from its baseline, which then
    /// becomes the new baseline.
    fn wait(&mut self) -> Result<(), String>;
}

#[derive(Debug, Error)]
pub enum DraftError {
    #[error("the draft has {0} problem(s)")]
    Problems(usize),
    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("watching the draft failed: {0}")]
    Watch(String),
}

/// One attempt to build a draft's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// The page, or every problem found, each as a line to show.
    pub page: Result<String, Vec<String>>,
    /// The files the build read or tried to read: the draft, and its base
    /// once the draft names one.
    pub sources: Vec<PathBuf>,
}

/// Loads the draft at `path` and its base, lays the draft over the base, and
/// builds the page. A base path is relative to the draft's folder.
pub fn build(path: &Path, files: &dyn DraftFiles) -> Build {
    let mut sources = vec![path.to_path_buf()];
    let page = load_draft(path, files).and_then(|draft| {
        let base = match &draft.base {
            DraftBase::Saved(base) => {
                let base_path = path.parent().unwrap_or(Path::new("")).join(base);
                sources.push(base_path.clone());
                Some(load_map(&base_path, files)?)
            }
            DraftBase::None { .. } => None,
        };
        let combined = map_draft::combine(&draft, base.as_ref()).map_err(|mismatch| {
            vec![format!(
                "{}: {}",
                path.display(),
                describe_mismatch(&mismatch)
            )]
        })?;
        if combined.problems.is_empty() {
            Ok(map_web::page(&combined.map))
        } else {
            Err(combined
                .problems
                .iter()
                .map(|problem| format!("{}: {}", path.display(), describe(problem)))
                .collect())
        }
    });
    Build { page, sources }
}

/// `srs draft check`: reports each problem, and fails if there are any.
pub fn check(draft: &Path, files: &mut dyn DraftFiles) -> Result<(), DraftError> {
    match build(draft, files).page {
        Ok(_) => {
            files.note(&format!("{}: no problems", draft.display()));
            Ok(())
        }
        Err(problems) => report(files, problems),
    }
}

/// `srs draft render`: writes the page to `out`, or reports each problem and
/// fails.
pub fn render(draft: &Path, out: &Path, files: &mut dyn DraftFiles) -> Result<(), DraftError> {
    match build(draft, files).page {
        Ok(page) => {
            files
                .write(out, &page)
                .map_err(|source| DraftError::Write {
                    path: out.to_path_buf(),
                    source,
                })?;
            files.note(&format!("wrote the draft's page to {}", out.display()));
            Ok(())
        }
        Err(problems) => report(files, problems),
    }
}

/// `srs draft preview`: builds the draft and shows the result, then waits
/// for its files to change and does it again. Returns only when watching
/// fails.
///
/// Each build reads files that are already watched, so an edit made while
/// they are read is noticed. A build that reads a new set of files, such as
/// when the draft names another base, watches the new set and builds again
/// before showing anything.
pub fn preview(
    draft: &Path,
    files: &dyn DraftFiles,
    sink: &dyn PreviewSink,
    watch: &mut dyn WatchFiles,
) -> DraftError {
    let mut watched = vec![draft.to_path_buf()];
    if let Err(error) = watch.watch(&watched) {
        return DraftError::Watch(error);
    }
    loop {
        let built = build(draft, files);
        if built.sources != watched {
            watched = built.sources;
            if let Err(error) = watch.watch(&watched) {
                return DraftError::Watch(error);
            }
            continue;
        }
        match built.page {
            Ok(page) => sink.show_page(page),
            Err(problems) => sink.show_problems(problems),
        }
        if let Err(error) = watch.wait() {
            return DraftError::Watch(error);
        }
    }
}

fn report(files: &mut dyn DraftFiles, problems: Vec<String>) -> Result<(), DraftError> {
    for problem in &problems {
        files.note(problem);
    }
    Err(DraftError::Problems(problems.len()))
}

fn load_draft(path: &Path, files: &dyn DraftFiles) -> Result<Draft, Vec<String>> {
    match load(path, files)? {
        MapDocument::Draft(draft) => Ok(draft),
        MapDocument::Map(_) => Err(vec![format!(
            "{}: is a saved map, not a draft",
            path.display()
        )]),
    }
}

fn load_map(path: &Path, files: &dyn DraftFiles) -> Result<ProjectMap, Vec<String>> {
    match load(path, files)? {
        MapDocument::Map(map) => Ok(map),
        MapDocument::Draft(_) => Err(vec![format!(
            "{}: is a draft, but a draft's base must be a saved map",
            path.display()
        )]),
    }
}

fn load(path: &Path, files: &dyn DraftFiles) -> Result<MapDocument, Vec<String>> {
    let text = files
        .read(path)
        .map_err(|error| vec![format!("{}: {error}", path.display())])?;
    map_persist::load(&text).map_err(|error| vec![located(path, &error)])
}

/// `path:line:column: message`, or `path: message` without a position.
fn located(path: &Path, error: &LoadError) -> String {
    match error.position {
        Some(position) => format!(
            "{}:{}:{}: {}",
            path.display(),
            position.line,
            position.column,
            error.message
        ),
        None => format!("{}: {}", path.display(), error.message),
    }
}

fn describe_mismatch(mismatch: &BaseMismatch) -> String {
    mismatch.to_string()
}

fn part(part: &MapPart) -> String {
    match part {
        MapPart::Module(id) => format!("module {}", id.0),
        MapPart::Dependency(id) => format!("the dependency of {} on {}", id.from.0, id.to.0),
        MapPart::Route(id) => format!("route {}", id.0),
    }
}

/// A draft problem in words.
pub fn describe(problem: &DraftProblem) -> String {
    match problem {
        DraftProblem::Collision { part: p } => {
            format!(
                "{} is already in the base map; add it under another key",
                part(p)
            )
        }
        DraftProblem::EvidenceCollision { citation } => {
            format!("evidence {} is already in the base map", citation.0)
        }
        DraftProblem::LayerCollision { layer } => {
            format!("layer {} is already listed by the base map", layer.0)
        }
        DraftProblem::UpdateOfMissingPart { part: p } => {
            format!("updates {}, which the base map does not have", part(p))
        }
        DraftProblem::RemovalOfMissingPart { part: p } => {
            format!("removes {}, which the base map does not have", part(p))
        }
        DraftProblem::ChangeOfProposal { part: p } => format!(
            "changes {}, which the base map already marks as a proposal",
            part(p)
        ),
        DraftProblem::IncompleteUpdate { part: p, missing } => format!(
            "updates {}, which the base map has not judged, without {}",
            part(p),
            missing.join(" or ")
        ),
        DraftProblem::UpdatedAndRemoved { part: p } => {
            format!("both updates and removes {}", part(p))
        }
        DraftProblem::Map(problem) => describe_map(problem),
    }
}

fn describe_map(problem: &MapProblem) -> String {
    match problem {
        MapProblem::MissingEvidence { part: p, citation } => {
            format!(
                "{} cites {}, which is not in the evidence",
                part(p),
                citation.0
            )
        }
        MapProblem::MissingModule { part: p, module } => {
            format!(
                "{} names module {}, which is not on the map",
                part(p),
                module.0
            )
        }
        MapProblem::MissingDependency {
            route,
            hop,
            dependency,
        } => format!(
            "route {} hop {} travels along the dependency of {} on {}, which is not on the map",
            route.0,
            hop + 1,
            dependency.from.0,
            dependency.to.0
        ),
        MapProblem::ObservedRestsOnAddition { part: p, addition } => format!(
            "{} is in code but rests on {}, which is a proposed addition",
            part(p),
            part(addition)
        ),
        MapProblem::ObservedCitesProposal { part: p, citation } => format!(
            "{} is in code but cites the proposal note {}",
            part(p),
            citation.0
        ),
        MapProblem::HopStatusMismatch { route, hop } => format!(
            "route {} hop {}: an added route's hops are proposed, and a route in code has none",
            route.0,
            hop + 1
        ),
        MapProblem::ProposedRestsOnRemoval { part: p, removed } => format!(
            "{} is proposed but rests on {}, which is proposed for removal",
            part(p),
            part(removed)
        ),
        MapProblem::UpdateChangesNothing { part: p } => {
            format!(
                "the update of {} reaches the same judgment as the review in code",
                part(p)
            )
        }
        MapProblem::UnlistedLayer { module, layer } => {
            format!(
                "module {} is in layer {}, which the map does not list",
                module.0, layer.0
            )
        }
    }
}

#[cfg(test)]
mod tests;
