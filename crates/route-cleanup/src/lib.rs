//! Passes a grapher runs over its routes once it has traced them. Neither
//! pass knows the language the graph came from.
//!
//! - [`merge`] merges routes that follow the same path, so a graph has one
//!   route per distinct path. See [`merging`] for the rules.
//! - [`exclude`] leaves out the routes that start in named outside packages
//!   and records what it left out. See [`exclusion`] for the rules.
//!
//! A grapher merges first and then excludes.

pub mod exclusion;
pub mod merging;

pub use exclusion::{Removed, exclude};
pub use merging::merge;
