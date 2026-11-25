//! Running `srs map`: graph, evaluate, build the map, then show it. Side
//! effects go through the injected implementations and outputs.

use crate::implementations::{EvaluatorChoice, Implementations, Language, Unavailable};
use graph_evaluator::EvaluationError;
use map_builder::{BuildProblem, GraphMismatch, MapBuild};
use project_graph::{BuildSelection, GraphingError};
use std::net::SocketAddr;
use std::path::PathBuf;
use thiserror::Error;

/// What `srs map` should map and where to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCommand {
    pub selection: BuildSelection,
    pub language: Language,
    pub evaluator: EvaluatorChoice,
    /// Where to write the map's page, if anywhere.
    pub page: Option<PathBuf>,
    /// Where to serve the map's page, if anywhere. Serving runs until the
    /// program is stopped.
    pub serve: Option<SocketAddr>,
}

/// Where a command's results go.
pub trait MapOutputs {
    /// Tells the person running the command something, such as a problem
    /// the map builder reported.
    fn note(&mut self, message: &str);
    fn write_page(&mut self, path: &std::path::Path, page: &str) -> std::io::Result<()>;
    /// Serves the page at `address`. Returns only when serving fails.
    fn serve_page(&mut self, address: SocketAddr, page: &str) -> std::io::Error;
}

#[derive(Debug, Error)]
pub enum MapError {
    #[error("no page to write or serve: give --page, --serve or both")]
    NothingToShow,
    #[error(transparent)]
    Unavailable(#[from] Unavailable),
    #[error("graphing failed: {0}")]
    Graphing(#[from] GraphingError),
    #[error("evaluation failed: {0}")]
    Evaluation(#[from] EvaluationError),
    #[error(transparent)]
    Mismatch(#[from] GraphMismatch),
    #[error("writing the page to {path}: {source}")]
    WritePage {
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
pub fn build(
    command: &MapCommand,
    implementations: &dyn Implementations,
) -> Result<MapBuild, MapError> {
    let grapher = implementations.grapher(command.language)?;
    let evaluator = implementations.evaluator(command.evaluator, &command.selection)?;
    let graph = grapher.build_graph(&command.selection)?;
    let evaluation = evaluator.evaluate(&graph)?;
    Ok(map_builder::build_map(&graph, &evaluation)?)
}

/// Runs `srs map`: builds the map, notes each problem the builder reported,
/// then writes the page and serves it as the command asks.
pub fn map(
    command: &MapCommand,
    implementations: &dyn Implementations,
    outputs: &mut dyn MapOutputs,
) -> Result<(), MapError> {
    if command.page.is_none() && command.serve.is_none() {
        return Err(MapError::NothingToShow);
    }
    let built = build(command, implementations)?;
    for problem in &built.problems {
        outputs.note(&describe(problem));
    }
    let page = map_web::page(&built.map);
    if let Some(path) = &command.page {
        outputs
            .write_page(path, &page)
            .map_err(|source| MapError::WritePage {
                path: path.clone(),
                source,
            })?;
        outputs.note(&format!("wrote the map page to {}", path.display()));
    }
    if let Some(address) = command.serve {
        outputs.note(&format!("serving the map at http://{address}/"));
        let source = outputs.serve_page(address, &page);
        return Err(MapError::Serve { address, source });
    }
    Ok(())
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
        BuildProblem::IncompleteGraph { cited_by, missing } => {
            format!("the graph's {cited_by:?} refers to {missing:?}, which the graph does not hold")
        }
    }
}

#[cfg(test)]
mod tests;
