//! Which source snapshot a graph describes.

use std::path::Path;
use std::process::Command;
use thiserror::Error;

pub trait SourceRevision {
    /// Identifies the source at `root`, including local changes.
    fn revision(&self, root: &Path) -> Result<String, RevisionError>;
}

#[derive(Debug, Error)]
#[error("could not identify the source revision: {0}")]
pub struct RevisionError(pub String);

/// The Git commit at `root`, with `-dirty` added when files under `root`
/// differ from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRevision {
    /// The Git executable, by path or by name on `PATH`.
    pub git: std::path::PathBuf,
}

impl SourceRevision for GitRevision {
    fn revision(&self, root: &Path) -> Result<String, RevisionError> {
        let commit = self.run(root, &["rev-parse", "HEAD"])?;
        let changes = self.run(root, &["status", "--porcelain", "--", "."])?;
        Ok(if changes.is_empty() {
            commit
        } else {
            format!("{commit}-dirty")
        })
    }
}

impl GitRevision {
    fn run(&self, root: &Path, args: &[&str]) -> Result<String, RevisionError> {
        let output = Command::new(&self.git)
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .map_err(|e| RevisionError(format!("{}: {e}", self.git.display())))?;
        if !output.status.success() {
            return Err(RevisionError(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
