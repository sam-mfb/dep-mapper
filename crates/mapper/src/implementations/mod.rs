//! The graphers and evaluators a command can select, and the contract for
//! supplying them. The program supplies the implementations that exist;
//! tests supply stand-ins.

use graph_evaluator::GraphEvaluator;
use project_graph::ProjectGrapher;
use thiserror::Error;

/// The language of the project to map. Each has its own grapher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
}

/// Who judges the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluatorChoice {
    /// A person, asked in the terminal.
    Manual,
    /// A language model.
    Llm,
}

/// Supplies the selected grapher and evaluator.
pub trait Implementations {
    fn grapher(&self, language: Language) -> Result<Box<dyn ProjectGrapher>, Unavailable>;
    fn evaluator(&self, choice: EvaluatorChoice) -> Result<Box<dyn GraphEvaluator>, Unavailable>;
}

/// The selected implementation cannot be supplied, such as one not built yet
/// or one missing its configuration.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct Unavailable(pub String);

/// The implementations in this build of the program.
pub struct Installed;

impl Implementations for Installed {
    fn grapher(&self, language: Language) -> Result<Box<dyn ProjectGrapher>, Unavailable> {
        let name = match language {
            Language::Rust => "project-graph-rust",
            Language::TypeScript => "project-graph-typescript",
        };
        Err(Unavailable(format!("{name} is not built yet")))
    }

    fn evaluator(&self, choice: EvaluatorChoice) -> Result<Box<dyn GraphEvaluator>, Unavailable> {
        let name = match choice {
            EvaluatorChoice::Manual => "graph-evaluator-manual",
            EvaluatorChoice::Llm => "graph-evaluator-llm",
        };
        Err(Unavailable(format!("{name} is not built yet")))
    }
}
