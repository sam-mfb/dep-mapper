//! An evaluator that an agent harness, such as Claude Code, or a person fills
//! out. The graph is presented as a workbook of pieces. Answers cite evidence
//! by written handles, such as `route:R12`, and are checked against the
//! workbook as they are recorded and again before the evaluation is built.
//!
//! This crate's core is pure: [`workbook::present`] turns a graph into a
//! workbook, [`answers::record`] applies one checked edit, and
//! [`checking::assemble`] turns the answers into a `GraphEvaluation`.
//! [`store`] keeps a workbook and its answers between commands, and
//! [`commands`] are what a reader runs to read pieces and record answers.

pub mod answers;
pub mod checking;
pub mod commands;
pub mod handles;
pub mod store;
pub mod workbook;

#[cfg(test)]
mod fixture;
