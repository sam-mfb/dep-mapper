//! Saves and loads project maps and drafts as TOML text. FORMAT.md describes
//! the file format for people who write drafts by hand. This crate owns the
//! encoding and its versions; reading and writing files is the caller's job.

mod decode;
mod encode;
mod format;

use project_map::{Draft, ProjectMap};

/// The contents of one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapDocument {
    /// A complete map, such as one saved after analysis.
    Map(ProjectMap),
    /// Proposed changes, laid over a saved map or standing alone.
    Draft(Draft),
}

/// Why a text could not be loaded, and where in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// `None` when the problem has no single place in the text.
    pub position: Option<Position>,
    pub message: String,
}

/// A place in the text. Both numbers start at 1; the column counts characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.position {
            Some(Position { line, column }) => write!(f, "{line}:{column}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads a map or draft file of a supported format version.
pub fn load(text: &str) -> Result<MapDocument, LoadError> {
    decode::document(text)
}

/// Writes a document in the current format version. `load` reads it back
/// unchanged.
pub fn save(document: &MapDocument) -> String {
    encode::document(document)
}

#[cfg(test)]
mod tests;
