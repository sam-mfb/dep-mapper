//! The `srs` program. It composes the other crates and owns no analysis,
//! judgment or presentation of its own.
//!
//! `srs map` builds the graph for a selected build with the language's
//! grapher, evaluates it with the selected evaluator, builds the map, saves
//! it with map-persist, and shows it: written as a page, served to a
//! browser, or both.
//!
//! `srs eval` runs the manual evaluator's answer commands on a workbook that
//! `srs map --evaluator manual` wrote.
//!
//! - [`run`] runs `srs map` against injected implementations and outputs.
//! - [`eval`] runs `srs eval`.
//! - [`implementations`] names the graphers and evaluators a command can
//!   select, and the contract for supplying them.
//! - [`serve`] serves one page over HTTP.
//! - [`cli`] turns command-line arguments into commands.

pub mod cli;
pub mod eval;
pub mod implementations;
pub mod run;
pub mod serve;
