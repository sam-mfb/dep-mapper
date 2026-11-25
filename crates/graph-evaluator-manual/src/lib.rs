//! An evaluator that an agent harness, such as Claude Code, or a person fills
//! out. The graph is presented as a workbook of pieces. Answers cite evidence
//! by written handles, such as `route:R12`, and are checked against the
//! workbook as they are recorded and again before the evaluation is built.
//!
//! Presenting, checking and assembling come from `graph-evaluator-pieces`
//! and are re-exported here under their own names.

pub use graph_evaluator_pieces::{answers, checking, handles, workbook};
