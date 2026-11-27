//! Builds the `ProjectGraph` for a TypeScript project: an npm workspace whose
//! packages are the graph's modules, with the build selected by a tsconfig.
//!
//! The TypeScript checker does the reading. `NodeCollector` runs the
//! collector in `collector/` (TypeScript 6, pinned by its lockfile) in a Node
//! child process; it prints items, facts and receiver links as JSON Lines.
//! This crate assembles those records into the graph and traces its value
//! routes with the shared `route-grapher` tracer.
//!
//! `TypeScriptGrapher` takes its collector, revision source and trace limits
//! at construction. Mapper builds it with `NodeCollector::bundled` and
//! `GitRevision`; tests can supply their own.

mod assembly;
pub mod collection;
mod grapher;
pub mod revision;
pub mod routes;

pub use collection::{CollectError, CollectRequest, Collector, NodeCollector};
pub use grapher::{AS_WRITTEN, TypeScriptGrapher};
pub use revision::{GitRevision, RevisionError, SourceRevision};
pub use routes::TraceLimits;
