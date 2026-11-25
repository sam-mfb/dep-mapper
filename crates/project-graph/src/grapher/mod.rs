//! The contract for building a `ProjectGraph` for a selected build. Each
//! language has its own implementation, which reads the source and traces the
//! routes itself. Route logic shared between languages belongs in a library
//! the implementations call, not in a separate stage of this contract.

use crate::{Build, ProjectGraph};
use std::path::PathBuf;
use thiserror::Error;

pub trait ProjectGrapher {
    /// Builds the graph for one selected build. The returned graph's `build`
    /// names the selected target and configuration, together with the project
    /// and source revision the implementation found at `root`.
    fn build_graph(&self, selection: &BuildSelection) -> Result<ProjectGraph, GraphingError>;
}

/// The build to analyze, in the terms of the project's language and build tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSelection {
    /// The directory that holds the project's source.
    pub root: PathBuf,
    /// For example, a Cargo package and target.
    pub target: String,
    /// The settings that select the build, such as features or a platform.
    pub configuration: String,
}

impl BuildSelection {
    /// Whether `build` names this selection's target and configuration.
    pub fn describes(&self, build: &Build) -> bool {
        build.target == self.target && build.configuration == self.configuration
    }
}

/// Failure to produce any graph for the selection, such as a build that
/// cannot be resolved. Parts of the build the implementation could not
/// analyze are coverage gaps in the returned graph, not errors.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct GraphingError {
    pub message: String,
}

/// A stand-in for consumers testing composition with a known graph.
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::*;

    pub struct FixedGrapher(pub ProjectGraph);

    impl ProjectGrapher for FixedGrapher {
        fn build_graph(&self, selection: &BuildSelection) -> Result<ProjectGraph, GraphingError> {
            if !selection.describes(&self.0.build) {
                return Err(GraphingError {
                    message: "fixture graph describes a different build".into(),
                });
            }
            Ok(self.0.clone())
        }
    }
}

#[cfg(test)]
mod tests;
