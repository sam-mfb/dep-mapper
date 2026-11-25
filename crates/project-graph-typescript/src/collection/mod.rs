//! Collecting source evidence from a TypeScript project. The `Collector`
//! contract hides how the TypeScript checker is reached; `NodeCollector`
//! runs the bundled collector script in a Node child process.

mod node;
pub mod records;

pub use node::NodeCollector;
use records::Record;
use std::path::PathBuf;
use thiserror::Error;

/// What to read: a project and the tsconfig that selects its build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectRequest {
    /// The project directory, holding the workspace's root package.json.
    pub root: PathBuf,
    /// The tsconfig file, relative to `root`.
    pub tsconfig: String,
}

pub trait Collector {
    /// Returns every record the collector produced, in order, ending with
    /// `Record::End`.
    fn collect(&self, request: &CollectRequest) -> Result<Vec<Record>, CollectError>;
}

#[derive(Debug, Error)]
pub enum CollectError {
    #[error("could not start the TypeScript collector: {0}")]
    Start(String),
    #[error("the TypeScript collector failed: {0}")]
    Failed(String),
    #[error("the TypeScript collector printed an unreadable record on line {line}: {message}")]
    Unreadable { line: usize, message: String },
}
