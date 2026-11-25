//! `ManualEvaluator` implements `GraphEvaluator` by handing a workbook to an
//! agent or a person and building the evaluation from their checked answers.
//!
//! `evaluate` presents the graph, saves the workbook, and asks the injected
//! `AgentRunner` to work through it. It returns once the runner returns and
//! the answers are submitted, after checking every answer again.
//!
//! A workbook for the same graph is resumed: its answers are kept, and if
//! they were already submitted the runner is not called again. A workbook
//! for a different graph is never overwritten.

use crate::checking::{self, Finding};
use crate::commands;
use crate::store::{StoreError, WorkbookStore};
use crate::workbook::{self, Presentation, SourceLines};
use graph_evaluator::{EvaluationError, GraphEvaluation, GraphEvaluator};
use project_graph::ProjectGraph;

/// Asks an agent or a person to work through a workbook.
pub trait AgentRunner {
    /// Returns when the worker has finished, whether or not they submitted.
    fn run(&self, task: &AgentTask) -> Result<(), RunError>;
}

/// Leaves the work to be done outside this run. The first `evaluate` writes
/// the workbook and reports that the answers are not submitted; a later
/// `evaluate`, after the answers are submitted, returns the evaluation.
pub struct ReturnAtOnce;

impl AgentRunner for ReturnAtOnce {
    fn run(&self, _task: &AgentTask) -> Result<(), RunError> {
        Ok(())
    }
}

/// What a runner hands to the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    /// Where the workbook is.
    pub location: String,
    /// A short request that points at the workbook and its instructions.
    pub prompt: String,
}

/// The runner could not get the work done, as distinct from work that was
/// done but not submitted.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct RunError {
    pub message: String,
}

/// How the evaluator presents a graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The most routes listed on one page of a piece.
    pub page_size: usize,
    /// How the worker invokes the answer commands, such as `srs eval`.
    pub command: String,
}

pub struct ManualEvaluator {
    store: Box<dyn WorkbookStore>,
    runner: Box<dyn AgentRunner>,
    source: Box<dyn SourceLines>,
    settings: Settings,
}

impl ManualEvaluator {
    pub fn new(
        store: Box<dyn WorkbookStore>,
        runner: Box<dyn AgentRunner>,
        source: Box<dyn SourceLines>,
        settings: Settings,
    ) -> ManualEvaluator {
        ManualEvaluator {
            store,
            runner,
            source,
            settings,
        }
    }

    fn prompt(&self) -> String {
        format!(
            "Evaluate the project graph in the workbook at {location}. Read {location}/INSTRUCTIONS.md first, then use `{c} index` and `{c} show` to read the pieces and the other `{c}` commands to record your answers. Finish with `{c} submit`.",
            location = self.store.location(),
            c = self.settings.command
        )
    }
}

impl GraphEvaluator for ManualEvaluator {
    fn evaluate(&self, graph: &ProjectGraph) -> Result<GraphEvaluation, EvaluationError> {
        let presentation = Presentation {
            page_size: self.settings.page_size,
            source: self.source.as_ref(),
            command: Some(&self.settings.command),
        };
        let workbook = workbook::present(graph, &presentation);
        let location = self.store.location();
        if let Some(existing) = self.store.load_workbook().map_err(store_failed)?
            && existing.graph != workbook.graph
        {
            return Err(failed(format!(
                "the workbook at {location} belongs to graph {}, not {}; move it aside or choose another location",
                existing.graph, workbook.graph
            )));
        }
        let instructions = commands::instructions(&self.settings.command, &workbook);
        self.store
            .save_workbook(&workbook, &instructions)
            .map_err(store_failed)?;
        if !self.store.submitted().map_err(store_failed)? {
            let task = AgentTask {
                location: location.clone(),
                prompt: self.prompt(),
            };
            self.runner
                .run(&task)
                .map_err(|e| failed(format!("the evaluation run failed: {e}")))?;
        }
        if !self.store.submitted().map_err(store_failed)? {
            return Err(failed(format!(
                "the answers at {location} were not submitted; they are kept, and evaluating again resumes them"
            )));
        }
        let answers = self.store.load_answers().map_err(store_failed)?;
        match checking::assemble(&workbook, &answers) {
            Ok(assembled) => Ok(assembled.evaluation),
            Err(problems) => Err(failed(format!(
                "the submitted answers at {location} have problems:\n{}",
                list(&problems)
            ))),
        }
    }
}

fn list(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("- {}", f.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn failed(message: String) -> EvaluationError {
    EvaluationError { message }
}

fn store_failed(error: StoreError) -> EvaluationError {
    failed(format!("the workbook store failed: {error}"))
}

#[cfg(test)]
mod tests;
