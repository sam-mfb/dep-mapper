//! The decision a human or model makes about a particular graph snapshot.
//! Evaluators receive the graph by reference and return judgments keyed to its
//! subjects. An omitted subject is unreviewed. An explicit deferral records
//! that the subject was considered and why it remains undecided.

use project_graph::{EvidenceId, GraphId, ItemId, ModuleId, ProjectGraph, RouteId};
use std::collections::BTreeMap;
use thiserror::Error;

pub trait GraphEvaluator {
    fn evaluate(&self, graph: &ProjectGraph) -> Result<GraphEvaluation, EvaluationError>;
}

/// Failure to obtain an evaluation, distinct from a decision to defer.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct EvaluationError {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphEvaluation {
    pub graph: GraphId,
    /// Layer names in the intended reading order.
    pub layers: Vec<String>,
    pub modules: BTreeMap<ModuleId, Decision<ModuleJudgment>>,
    pub routes: BTreeMap<RouteId, Decision<RouteJudgment>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision<T> {
    Judged {
        judgment: T,
        reason: String,
        primary: Citation,
        supporting: Vec<Citation>,
    },
    Deferred {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Citation {
    Declaration(ItemId),
    Fact(EvidenceId),
    Route(RouteId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleJudgment {
    pub concern: String,
    pub kind: ModuleKind,
    /// Names an entry in the evaluation's layer order when assigned.
    pub layer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Logic,
    Effect,
    Composition,
    Contract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteJudgment {
    pub role: UseKind,
    pub other_roles: Vec<UseKind>,
    /// Optional roles for individual handoffs, keyed by zero-based hop index.
    pub hops: BTreeMap<usize, UseKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseKind {
    DirectUse,
    Implements,
    Wires,
    SuppliesInput,
    FullInjection,
    AdaptedInjection,
    Incidental,
    Other(String),
}

/// A stand-in for consumers testing composition with a known graph and answer.
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::*;

    pub struct FixedEvaluator(pub GraphEvaluation);

    impl GraphEvaluator for FixedEvaluator {
        fn evaluate(&self, graph: &ProjectGraph) -> Result<GraphEvaluation, EvaluationError> {
            if self.0.graph != graph.id {
                return Err(EvaluationError {
                    message: "fixture evaluation belongs to a different graph".into(),
                });
            }
            Ok(self.0.clone())
        }
    }
}

#[cfg(test)]
mod tests;
