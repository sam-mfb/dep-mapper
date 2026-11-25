//! The decision a human or model makes about a particular graph snapshot.
//! Evaluators receive the graph by reference. They judge each module, and
//! interpret the graph's evidence as relationships: what it means in the
//! architecture. The graph records how modules are joined in the code; a
//! relationship records what that means, and can draw on several routes. One
//! route can carry several relationships.
//!
//! A module the evaluation does not mention is unreviewed. An explicit
//! deferral records that it was considered and why it remains undecided.
//! How an evaluator records what it considered and set aside is its own
//! concern.

mod clusters;

pub use clusters::{Clusters, DuplicateCluster};
use project_graph::{GraphId, GraphRef, ModuleId, ProjectGraph};
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
    /// Cluster names in the intended reading order.
    pub clusters: Clusters,
    pub modules: BTreeMap<ModuleId, Decision<ModuleJudgment>>,
    /// Relationships found in the graph's evidence. A route can carry several
    /// relationships or none.
    pub relationships: Vec<Relationship>,
}

/// Names one of the evaluation's clusters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterName(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision<T> {
    Judged {
        judgment: T,
        /// Why the judgment was made, including how the cited evidence bears on it.
        reason: String,
        /// The things in the graph the judgment rests on.
        evidence: Vec<GraphRef>,
    },
    Deferred {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleJudgment {
    pub concern: String,
    pub kind: ModuleKind,
    /// Names an entry in the evaluation's clusters.
    pub cluster: ClusterName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Logic,
    Effect,
    Composition,
    Contract,
}

/// What some of the graph's evidence means in the architecture. The routes it
/// cites are the ones it interprets. For example, "B adapts A for C" cites the
/// route from A through B to C and the facts inside B that do the adapting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub kind: RelationshipKind,
    /// Why the evidence means this, including how each citation bears on it.
    pub reason: String,
    pub evidence: Vec<GraphRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationshipKind {
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
