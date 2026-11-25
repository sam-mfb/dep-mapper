//! Test helpers: the shared fixture graph and scratch directories.

pub use graph_evaluator_pieces::testing::graph;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh directory under the system's temporary directory, removed on drop.
pub struct Scratch(pub std::path::PathBuf);

impl Scratch {
    pub fn new() -> Scratch {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "srs-eval-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
