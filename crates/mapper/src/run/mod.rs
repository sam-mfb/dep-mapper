//! Running `srs map`: graph, evaluate, build the map, save it, then show it.
//! Side effects go through the injected implementations, files and outputs.

use crate::draft::{self, DraftFiles};
use crate::implementations::{EvaluatorChoice, Implementations, Language, Unavailable};
use graph_evaluation_assembly::workbook::ClusterPlan;
use graph_evaluator::EvaluationError;
use map_builder::{BuildProblem, GraphMismatch, MapBuild};
use map_persist::MapDocument;
use project_graph::{BuildSelection, GraphingError};
use project_map::{ProjectMap, Review, Status};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// What `srs map` should map, where to save it and where to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCommand {
    pub selection: BuildSelection,
    pub language: Language,
    pub evaluator: EvaluatorChoice,
    /// Where the map's layers come from.
    pub layers: LayerSource,
    /// Where to save the map, so it can be reopened without analysis.
    pub out: PathBuf,
    /// Where to write the map's page, if anywhere.
    pub page: Option<PathBuf>,
    /// Where to serve the map's page, if anywhere. Serving runs until the
    /// program is stopped.
    pub serve: Option<SocketAddr>,
}

/// Where `srs map` gets the map's layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayerSource {
    /// The evaluator proposes them.
    Proposed,
    /// Fixed in advance, from top to bottom.
    Listed(Vec<String>),
    /// Fixed in advance by the draft at this path, laid over its base map:
    /// the combined map's layers. Each module's layer there is shown to the
    /// evaluator as a hint.
    Draft(PathBuf),
}

/// Where a command's results go.
pub trait MapOutputs {
    /// Tells the person running the command something, such as a problem
    /// the map builder reported.
    fn note(&mut self, message: &str);
    /// Writes `text` to the file at `path`, replacing what was there.
    fn write_file(&mut self, path: &std::path::Path, text: &str) -> std::io::Result<()>;
    /// Serves the page at `address`, saying where once it is listening.
    /// Returns only when serving fails, including when it cannot start.
    fn serve_page(&mut self, address: SocketAddr, page: &str) -> std::io::Error;
}

#[derive(Debug, Error)]
pub enum MapError {
    #[error(transparent)]
    Unavailable(#[from] Unavailable),
    #[error("graphing failed: {0}")]
    Graphing(#[from] GraphingError),
    #[error("evaluation failed: {0}")]
    Evaluation(#[from] EvaluationError),
    #[error(transparent)]
    Mismatch(#[from] GraphMismatch),
    #[error("the draft that gives the layers cannot be used:\n{}", .0.join("\n"))]
    Layers(Vec<String>),
    #[error("writing {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("serving the page at {address}: {source}")]
    Serve {
        address: SocketAddr,
        source: std::io::Error,
    },
}

/// Graphs, evaluates and builds the map for the command's selection.
/// `files` holds the draft, when the layers come from one.
pub fn build(
    command: &MapCommand,
    implementations: &dyn Implementations,
    files: &dyn DraftFiles,
) -> Result<MapBuild, MapError> {
    // The layers and the evaluator first: their checks are quick, while a
    // grapher may build tools before it can start.
    let clusters = cluster_plan(&command.layers, files)?;
    let evaluator = implementations.evaluator(&command.evaluator, &command.selection, &clusters)?;
    let grapher = implementations.grapher(&command.language)?;
    let graph = grapher.build_graph(&command.selection)?;
    let evaluation = evaluator.evaluate(&graph)?;
    Ok(map_builder::build_map(&graph, &evaluation)?)
}

/// Runs `srs map`: builds the map, notes each problem the builder reported,
/// saves the map, then writes the page and serves it as the command asks.
pub fn map(
    command: &MapCommand,
    implementations: &dyn Implementations,
    files: &dyn DraftFiles,
    outputs: &mut dyn MapOutputs,
) -> Result<(), MapError> {
    let built = build(command, implementations, files)?;
    for problem in &built.problems {
        outputs.note(&describe(problem));
    }
    let saved = map_persist::save(&MapDocument::Map(built.map.clone()));
    write(outputs, &command.out, &saved)?;
    outputs.note(&format!("saved the map to {}", command.out.display()));
    present(&built.map, command.page.as_deref(), command.serve, outputs)
}

/// What the evaluator is told about layers, from the command's source.
fn cluster_plan(source: &LayerSource, files: &dyn DraftFiles) -> Result<ClusterPlan, MapError> {
    match source {
        LayerSource::Proposed => Ok(ClusterPlan::proposed()),
        LayerSource::Listed(layers) => Ok(ClusterPlan {
            fixed: Some(layers.clone()),
            previous: BTreeMap::new(),
        }),
        LayerSource::Draft(path) => {
            let map = draft::laid_over(path, files).map_err(MapError::Layers)?;
            if map.layers.is_empty() {
                return Err(MapError::Layers(vec![format!(
                    "{}: the draft and its base list no layers",
                    path.display()
                )]));
            }
            Ok(ClusterPlan {
                fixed: Some(map.layers.iter().map(|layer| layer.0.clone()).collect()),
                previous: planned_layers(&map),
            })
        }
    }
}

/// Each module's layer in a map, by module identifier: its proposed layer
/// when the map proposes a new review, otherwise its reviewed one.
fn planned_layers(map: &ProjectMap) -> BTreeMap<String, String> {
    map.modules
        .iter()
        .filter_map(|(id, module)| {
            let review = match &module.status {
                Status::ProposedUpdate { proposed } => proposed,
                _ => &module.description,
            };
            match review {
                Review::Judged { judgment, .. } => {
                    Some((id.0.clone(), judgment.layer.as_ref()?.0.clone()))
                }
                Review::Unreviewed | Review::Deferred { .. } => None,
            }
        })
        .collect()
}

/// Writes the map's page to `page` and serves it at `serve`, each if given.
/// Serving runs until it fails.
pub fn present(
    map: &ProjectMap,
    page: Option<&Path>,
    serve: Option<SocketAddr>,
    outputs: &mut dyn MapOutputs,
) -> Result<(), MapError> {
    if page.is_none() && serve.is_none() {
        return Ok(());
    }
    let text = map_web::page(map);
    if let Some(path) = page {
        write(outputs, path, &text)?;
        outputs.note(&format!("wrote the map page to {}", path.display()));
    }
    if let Some(address) = serve {
        let source = outputs.serve_page(address, &text);
        return Err(MapError::Serve { address, source });
    }
    Ok(())
}

fn write(outputs: &mut dyn MapOutputs, path: &std::path::Path, text: &str) -> Result<(), MapError> {
    outputs
        .write_file(path, text)
        .map_err(|source| MapError::WriteFile {
            path: path.into(),
            source,
        })
}

fn describe(problem: &BuildProblem) -> String {
    match problem {
        BuildProblem::UnknownModule(module) => format!(
            "the evaluation judges module {}, which the graph does not have",
            module.0
        ),
        BuildProblem::UnresolvedModuleCitation { module, reference } => format!(
            "the judgment of module {} cites {reference:?}, which the graph does not have",
            module.0
        ),
        BuildProblem::UnlistedCluster { module, cluster } => format!(
            "module {} is placed in cluster {}, which the evaluation does not list",
            module.0, cluster.0
        ),
        BuildProblem::UnresolvedRelationshipCitation {
            relationship,
            reference,
        } => format!(
            "relationship {relationship} cites {reference:?}, which the graph does not have"
        ),
        BuildProblem::UnplacedRelationship { relationship } => {
            format!("relationship {relationship} applies to no route, hop or dependency on the map")
        }
        BuildProblem::DuplicateCluster(cluster) => format!(
            "the evaluation lists cluster {} more than once; the map keeps its first place",
            cluster.0
        ),
        BuildProblem::IncompleteGraph { cited_by, missing } => {
            format!("the graph's {cited_by:?} refers to {missing:?}, which the graph does not hold")
        }
    }
}

#[cfg(test)]
mod tests;
