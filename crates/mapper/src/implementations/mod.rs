//! The graphers and evaluators a command can select, and the contract for
//! supplying them. The program supplies the implementations that exist;
//! tests supply stand-ins.

use graph_evaluator::GraphEvaluator;
use graph_evaluator_manual::evaluator::{ManualEvaluator, ReturnAtOnce, Settings};
use graph_evaluator_manual::store::DirStore;
use graph_evaluator_manual::workbook::ByteOffsets;
use project_graph::{BuildSelection, ProjectGrapher};
use project_graph_typescript::{GitRevision, NodeCollector, TraceLimits, TypeScriptGrapher};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The language of the project to map. Each has its own grapher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
}

/// Who judges the graph, and what they need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluatorChoice {
    /// A person or agent, answering in a workbook with `srs eval`.
    Manual {
        /// The directory holding the workbook and its answers.
        workbook: PathBuf,
    },
    /// A language model.
    Llm,
}

/// Supplies the selected grapher and evaluator. Both are asked for before
/// any analysis starts, so a missing piece or setting fails early.
pub trait Implementations {
    fn grapher(&self, language: Language) -> Result<Box<dyn ProjectGrapher>, Unavailable>;
    /// `selection` is the build being mapped, for evaluators that read the
    /// project's source.
    fn evaluator(
        &self,
        choice: &EvaluatorChoice,
        selection: &BuildSelection,
    ) -> Result<Box<dyn GraphEvaluator>, Unavailable>;
}

/// The selected implementation cannot be supplied, such as one not built yet
/// or one missing its configuration.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct Unavailable(pub String);

/// The most routes on one page of a workbook piece.
pub const WORKBOOK_PAGE_SIZE: usize = 40;

/// How a person or agent runs the answer commands for the workbook at
/// `workbook`.
pub fn eval_command(workbook: &Path) -> String {
    format!("srs eval --workbook {}", workbook.display())
}

/// The implementations in this build of the program, with the tools they
/// run found on `PATH`.
pub struct Installed;

impl Implementations for Installed {
    fn grapher(&self, language: Language) -> Result<Box<dyn ProjectGrapher>, Unavailable> {
        match language {
            Language::Rust => Err(Unavailable("project-graph-rust is not built yet".into())),
            Language::TypeScript => Ok(Box::new(TypeScriptGrapher::new(
                Box::new(NodeCollector::bundled("node".into())),
                Box::new(GitRevision { git: "git".into() }),
                TraceLimits::STANDARD,
            ))),
        }
    }

    fn evaluator(
        &self,
        choice: &EvaluatorChoice,
        _selection: &BuildSelection,
    ) -> Result<Box<dyn GraphEvaluator>, Unavailable> {
        match choice {
            EvaluatorChoice::Manual { workbook } => Ok(Box::new(ManualEvaluator::new(
                Box::new(DirStore::new(workbook.clone())),
                Box::new(ReturnAtOnce),
                Box::new(ByteOffsets),
                Settings {
                    page_size: WORKBOOK_PAGE_SIZE,
                    command: eval_command(workbook),
                },
            ))),
            EvaluatorChoice::Llm => Err(Unavailable("graph-evaluator-llm is not built yet".into())),
        }
    }
}
