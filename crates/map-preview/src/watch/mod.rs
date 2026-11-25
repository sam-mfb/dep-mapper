//! Noticing when files change.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use thiserror::Error;

/// Waits for files to change.
pub trait Watcher {
    /// Blocks until at least one watched file changes, is created or is
    /// removed, and returns those that did.
    fn wait_for_change(&mut self) -> Result<Vec<PathBuf>, WatchError>;
}

#[derive(Debug, Error)]
#[error("cannot watch {path}: {source}")]
pub struct WatchError {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

/// Watches files by reading their modification time and size at a fixed
/// interval. It needs no library or operating-system support.
pub struct PollingWatcher {
    interval: Duration,
    seen: BTreeMap<PathBuf, Option<Stamp>>,
}

/// What reading a file's metadata shows; `None` in `seen` means missing.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: SystemTime,
    len: u64,
}

impl PollingWatcher {
    /// Starts watching `paths` as they are now. A missing file counts as a
    /// change when it appears.
    pub fn new(paths: Vec<PathBuf>, interval: Duration) -> Result<Self, WatchError> {
        let seen = paths
            .into_iter()
            .map(|path| Ok((path.clone(), stamp(&path)?)))
            .collect::<Result<_, WatchError>>()?;
        Ok(PollingWatcher { interval, seen })
    }
}

impl Watcher for PollingWatcher {
    fn wait_for_change(&mut self) -> Result<Vec<PathBuf>, WatchError> {
        loop {
            std::thread::sleep(self.interval);
            let mut changed = vec![];
            for (path, last) in &mut self.seen {
                let now = stamp(path)?;
                if now != *last {
                    *last = now;
                    changed.push(path.clone());
                }
            }
            if !changed.is_empty() {
                return Ok(changed);
            }
        }
    }
}

fn stamp(path: &PathBuf) -> Result<Option<Stamp>, WatchError> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let modified = metadata.modified().map_err(|source| WatchError {
                path: path.clone(),
                source,
            })?;
            Ok(Some(Stamp {
                modified,
                len: metadata.len(),
            }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(WatchError {
            path: path.clone(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests;
