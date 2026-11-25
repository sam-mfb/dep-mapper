//! The graphers and evaluators a command can select, and the contract for
//! supplying them. The program supplies the implementations that exist;
//! tests supply stand-ins.

use graph_evaluator::GraphEvaluator;
use graph_evaluator_manual::checkout::CheckoutLines;
use graph_evaluator_manual::evaluator::{AgentRunner, ManualEvaluator, ReturnAtOnce, Settings};
use graph_evaluator_manual::runners::{
    ClaudeCode, SystemPause, SystemProcesses, WaitForSubmission, suggested_claude_args,
};
use graph_evaluator_manual::store::DirStore;
use project_graph::{BuildSelection, ProjectGrapher};
use project_graph_typescript::{GitRevision, NodeCollector, TraceLimits, TypeScriptGrapher};
use std::path::{Path, PathBuf};
use std::time::Duration;
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
        /// Who answers it while `srs map` runs.
        agent: Agent,
    },
    /// A language model.
    Llm,
}

/// Who answers a manual evaluation's workbook while `srs map` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    /// No one: `srs map` writes the workbook and stops. Run it again after
    /// the answers are submitted.
    None,
    /// A person or an agent started by hand: `srs map` shows the request and
    /// waits for the submission.
    Wait,
    /// Claude Code, run in the project directory. `srs` must be on `PATH`
    /// so it can run `srs eval`.
    Claude,
}

/// How often `--agent wait` checks for a submission.
pub const WAIT_INTERVAL: Duration = Duration::from_secs(5);
/// How long `--agent wait` waits before stopping with the answers kept.
pub const WAIT_LIMIT: Duration = Duration::from_secs(4 * 60 * 60);

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
        selection: &BuildSelection,
    ) -> Result<Box<dyn GraphEvaluator>, Unavailable> {
        match choice {
            EvaluatorChoice::Manual { workbook, agent } => {
                let runner: Box<dyn AgentRunner> = match agent {
                    Agent::None => Box::new(ReturnAtOnce),
                    Agent::Wait => Box::new(
                        WaitForSubmission::new(
                            Box::new(DirStore::new(workbook.clone())),
                            Box::new(std::io::stderr()),
                            Box::new(SystemPause),
                            WAIT_INTERVAL,
                            WAIT_LIMIT,
                        )
                        .map_err(|e| Unavailable(e.to_string()))?,
                    ),
                    Agent::Claude => Box::new(ClaudeCode::new(
                        Box::new(SystemProcesses),
                        "claude".into(),
                        suggested_claude_args(),
                        selection.root.clone(),
                    )),
                };
                Ok(Box::new(ManualEvaluator::new(
                    Box::new(DirStore::new(workbook.clone())),
                    runner,
                    Box::new(CheckoutLines::new(selection.root.clone())),
                    Settings {
                        page_size: WORKBOOK_PAGE_SIZE,
                        command: eval_command(workbook),
                    },
                )))
            }
            EvaluatorChoice::Llm => Err(Unavailable("graph-evaluator-llm is not built yet".into())),
        }
    }
}
