//! Which source snapshot a graph describes.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

pub trait SourceRevision {
    /// Identifies the source at `root`, including local changes.
    fn revision(&self, root: &Path) -> Result<String, RevisionError>;
}

#[derive(Debug, Error)]
#[error("could not identify the source revision: {0}")]
pub struct RevisionError(pub String);

/// The Git commit at `root`. When files under `root` differ from it, the
/// revision is `<commit>-dirty-<hash>`, where the hash covers the changes to
/// tracked files and the contents of untracked ones, so different local
/// changes give different revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRevision {
    /// The Git executable, by path or by name on `PATH`.
    pub git: PathBuf,
}

impl SourceRevision for GitRevision {
    fn revision(&self, root: &Path) -> Result<String, RevisionError> {
        let commit = text(self.run(root, &["rev-parse", "HEAD"])?);
        let changes = self.run(root, &["status", "--porcelain", "--", "."])?;
        if changes.is_empty() {
            return Ok(commit);
        }
        let mut hash = Sha256::new();
        hash.update(self.run(root, &["diff", "HEAD", "--binary", "--", "."])?);
        let untracked = self.run(
            root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                ".",
            ],
        )?;
        for path in untracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let name = String::from_utf8_lossy(path).to_string();
            let contents = std::fs::read(root.join(&name))
                .map_err(|e| RevisionError(format!("{name}: {e}")))?;
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((contents.len() as u64).to_le_bytes());
            hash.update(&contents);
        }
        let digest: String = hash
            .finalize()
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(format!("{commit}-dirty-{digest}"))
    }
}

impl GitRevision {
    fn run(&self, root: &Path, args: &[&str]) -> Result<Vec<u8>, RevisionError> {
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
        Ok(output.stdout)
    }
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_string()
}

#[cfg(test)]
mod tests;
