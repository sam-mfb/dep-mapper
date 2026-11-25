//! What an evaluator needs to have a graph judged piece by piece, whoever
//! does the judging. It is pure: no files, processes or network.
//!
//! - [`workbook::present`] cuts a graph into pieces with readable text: one
//!   per module, one for the cluster list, and one per pair of modules that
//!   routes join. It also indexes every handle an answer may cite.
//! - [`handles`] are the written names that answers cite, such as
//!   `route:R12`. Only this crate turns them into `GraphRef`s.
//! - [`answers::record`] applies one edit to the answers and refuses it if
//!   the checks find a problem with what it changed.
//! - [`checking`] lists problems and warnings, reports progress, and
//!   assembles the answers into a `GraphEvaluation`.

pub mod answers;
pub mod checking;
pub mod handles;
pub mod workbook;

/// A small graph for consumers' tests.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
