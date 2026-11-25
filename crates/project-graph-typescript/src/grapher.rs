use crate::assembly::{Selected, assemble};
use crate::collection::{CollectRequest, Collector};
use crate::revision::SourceRevision;
use crate::routes::TraceLimits;
use project_graph::{BuildSelection, GraphingError, ProjectGraph, ProjectGrapher};

/// The only configuration this grapher accepts: the compiler options exactly
/// as the selected tsconfig states them.
pub const AS_WRITTEN: &str = "tsconfig";

/// Builds the graph for an npm workspace of TypeScript packages.
///
/// A selection's `target` is the tsconfig file, relative to the project
/// root, that names the source files and compiler options. Its
/// `configuration` must be [`AS_WRITTEN`].
pub struct TypeScriptGrapher {
    collector: Box<dyn Collector>,
    revision: Box<dyn SourceRevision>,
    limits: TraceLimits,
}

impl TypeScriptGrapher {
    pub fn new(
        collector: Box<dyn Collector>,
        revision: Box<dyn SourceRevision>,
        limits: TraceLimits,
    ) -> Self {
        TypeScriptGrapher {
            collector,
            revision,
            limits,
        }
    }
}

impl ProjectGrapher for TypeScriptGrapher {
    fn build_graph(&self, selection: &BuildSelection) -> Result<ProjectGraph, GraphingError> {
        if selection.configuration != AS_WRITTEN {
            return Err(GraphingError {
                message: format!(
                    "the TypeScript grapher reads the tsconfig as written; configuration must be \"{AS_WRITTEN}\", not \"{}\"",
                    selection.configuration
                ),
            });
        }
        let failed = |e: &dyn std::fmt::Display| GraphingError {
            message: e.to_string(),
        };
        let source_revision = self
            .revision
            .revision(&selection.root)
            .map_err(|e| failed(&e))?;
        let records = self
            .collector
            .collect(&CollectRequest {
                root: selection.root.clone(),
                tsconfig: selection.target.clone(),
            })
            .map_err(|e| failed(&e))?;
        let selected = Selected {
            source_revision,
            target: selection.target.clone(),
            configuration: selection.configuration.clone(),
        };
        assemble(&selected, records, self.limits).map_err(|e| failed(&e))
    }
}
