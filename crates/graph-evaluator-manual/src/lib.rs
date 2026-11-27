//! An evaluator that an agent harness, such as Claude Code, or a person fills
//! out. The graph is presented as a workbook of pieces. Answers cite evidence
//! by written handles, such as `route:R12`, and are checked against the
//! workbook as they are recorded and again before the evaluation is built.
//!
//! Presenting, checking and assembling come from `graph-evaluation-assembly`
//! and are re-exported here under their own names: [`workbook::present`],
//! [`answers::record`] and [`checking::assemble`].
//! [`store`] keeps a workbook and its answers between commands, and
//! [`commands`] are what a reader runs to read pieces and record answers.
//! [`evaluator::ManualEvaluator`] ties these together behind the
//! `GraphEvaluator` contract, with the worker behind an injected
//! [`evaluator::AgentRunner`]. [`runners`] has runners that wait for a
//! person or start Claude Code, and [`checkout::CheckoutLines`] reads line
//! numbers from the project checkout.

pub use graph_evaluation_assembly::{answers, checking, handles, workbook};

pub mod checkout;
pub mod commands;
pub mod evaluator;
pub mod runners;
pub mod store;

#[cfg(test)]
mod fixture;
