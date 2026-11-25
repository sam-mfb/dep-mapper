//! Running `srs map`: graph, evaluate, build the map, save it, then show it.
//! Side effects go through the injected implementations and outputs.

use crate::implementations::{EvaluatorChoice, Implementations, Language, Unavailable};
use graph_evaluator::EvaluationError;
use map_builder::{BuildProblem, GraphMismatch, MapBuild};
use map_persist::MapDocument;
use project_graph::{BuildSelection, GraphingError};
use std::net::SocketAddr;
use std::path::PathBuf;
use thiserror::Error;

/// What `srs map` should map, where to save it and where to show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCommand {
    pub selection: BuildSelection,
    pub language: Language,
    pub evaluator: EvaluatorChoice,
    /// Where to save the map, so it can be reopened without analysis.
    pub out: PathBuf,
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
pub fn build(
    command: &MapCommand,
    implementations: &dyn Implementations,
) -> Result<MapBuild, MapError> {
    let grapher = implementations.grapher(command.language)?;
    let evaluator = implementations.evaluator(&command.evaluator, &command.selection)?;
    let graph = grapher.build_graph(&command.selection)?;
    let evaluation = evaluator.evaluate(&graph)?;
    Ok(map_builder::build_map(&graph, &evaluation)?)
}

/// Runs `srs map`: builds the map, notes each problem the builder reported,
/// saves the map, then writes the page and serves it as the command asks.
pub fn map(
    command: &MapCommand,
    implementations: &dyn Implementations,
    outputs: &mut dyn MapOutputs,
) -> Result<(), MapError> {
    let built = build(command, implementations)?;
    for problem in &built.problems {
        outputs.note(&describe(problem));
    }
    let page = map_web::page(&built.map);
    let saved = map_persist::save(&MapDocument::Map(built.map));
    write(outputs, &command.out, &saved)?;
    outputs.note(&format!("saved the map to {}", command.out.display()));
    if let Some(path) = &command.page {
        write(outputs, path, &page)?;
        outputs.note(&format!("wrote the map page to {}", path.display()));
    }
    if let Some(address) = command.serve {
        let source = outputs.serve_page(address, &page);
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
        BuildProblem::IncompleteGraph { cited_by, missing } => {
            format!("the graph's {cited_by:?} refers to {missing:?}, which the graph does not hold")
        }
    }
}

#[cfg(test)]
mod tests;
