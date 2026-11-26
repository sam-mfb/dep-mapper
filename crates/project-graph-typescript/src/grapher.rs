use crate::assembly::{Selected, assemble};
use crate::collection::records::Record;
use crate::collection::{CollectRequest, Collector};
use crate::revision::{RevisionError, SourceRevision};
use crate::routes::TraceLimits;
use project_graph::{BuildSelection, GraphingError, ProjectGraph, ProjectGrapher};

/// The only configuration this grapher accepts: the compiler options exactly
/// as the selected tsconfig states them.
pub const AS_WRITTEN: &str = "tsconfig";

/// Builds the graph for an npm workspace of TypeScript packages.
///
/// A selection's `target` is the tsconfig file, relative to the project
/// root, that names the source files and compiler options. Its
/// `configuration` must be [`AS_WRITTEN`].
///
/// The revision comes from the injected [`SourceRevision`]. When it reports
/// that the source is not under version control, the revision is
/// `unversioned-<hash>`, from the contents of the files the collector read.
pub struct TypeScriptGrapher {
    collector: Box<dyn Collector>,
    revision: Box<dyn SourceRevision>,
    limits: TraceLimits,
    excluded: Vec<String>,
}

impl TypeScriptGrapher {
    /// `excluded` names outside packages, such as `lodash` or `@types/node`,
    /// whose routes the graph leaves out; the graph records the list and how
    /// much it removed. Empty keeps every route.
    pub fn new(
        collector: Box<dyn Collector>,
        revision: Box<dyn SourceRevision>,
        limits: TraceLimits,
        excluded: Vec<String>,
    ) -> Self {
        TypeScriptGrapher {
            collector,
            revision,
            limits,
            excluded,
        }
    }
}

impl ProjectGrapher for TypeScriptGrapher {
    fn build_graph(&self, selection: &BuildSelection) -> Result<ProjectGraph, GraphingError> {
        if selection.configuration != AS_WRITTEN {
            return Err(GraphingError {
                message: format!(
                    "the TypeScript grapher reads the tsconfig as written; configuration must be \"{AS_WRITTEN}\", not \"{}\"",
                    selection.configuration
                ),
            });
        }
        let failed = |e: &dyn std::fmt::Display| GraphingError {
            message: e.to_string(),
        };
        let versioned = match self.revision.revision(&selection.root) {
            Ok(revision) => Some(revision),
            Err(RevisionError::NotVersioned) => None,
            Err(e) => return Err(failed(&e)),
        };
        let records = self
            .collector
            .collect(&CollectRequest {
                root: selection.root.clone(),
                tsconfig: selection.target.clone(),
            })
            .map_err(|e| failed(&e))?;
        let source_revision = versioned.unwrap_or_else(|| unversioned(&records));
        let selected = Selected {
            source_revision,
            target: selection.target.clone(),
            configuration: selection.configuration.clone(),
        };
        assemble(&selected, records, self.limits, &self.excluded).map_err(|e| failed(&e))
    }
}

/// The revision of source outside version control: `unversioned-` and the
/// first 16 hex digits of the hash of the files the collector read. Output
/// without a project record is refused when the graph is assembled.
fn unversioned(records: &[Record]) -> String {
    let sources = records.iter().find_map(|record| match record {
        Record::Project { sources, .. } => Some(sources.as_str()),
        _ => None,
    });
    match sources {
        Some(sources) => format!("unversioned-{}", &sources[..sources.len().min(16)]),
        None => "unversioned".to_string(),
    }
}
