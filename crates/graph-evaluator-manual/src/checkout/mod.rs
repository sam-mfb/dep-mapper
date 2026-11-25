//! Line numbers for source spans, read from a project checkout.

use crate::workbook::SourceLines;
use project_graph::SourceSpan;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

/// Reads files under `root` to turn byte offsets into line numbers. A file
/// that cannot be read, or a span beyond its end, gives no lines, so the
/// span is shown by its byte offsets.
pub struct CheckoutLines {
    root: PathBuf,
    /// The byte offset where each line starts, by file, read once.
    starts: RefCell<HashMap<String, Option<Vec<usize>>>>,
}

impl CheckoutLines {
    pub fn new(root: impl Into<PathBuf>) -> CheckoutLines {
        CheckoutLines {
            root: root.into(),
            starts: RefCell::new(HashMap::new()),
        }
    }
}

impl SourceLines for CheckoutLines {
    fn lines(&self, span: &SourceSpan) -> Option<(usize, usize)> {
        let mut cache = self.starts.borrow_mut();
        let starts = cache
            .entry(span.file.clone())
            .or_insert_with(|| {
                let bytes = std::fs::read(self.root.join(&span.file)).ok()?;
                let mut starts = vec![0];
                starts.extend(
                    bytes
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| **b == b'\n')
                        .map(|(i, _)| i + 1),
                );
                starts.push(bytes.len() + 1);
                Some(starts)
            })
            .as_ref()?;
        let end_of_file = *starts.last()? - 1;
        if span.bytes.end > end_of_file || span.bytes.start > span.bytes.end {
            return None;
        }
        let line = |offset: usize| starts.partition_point(|start| *start <= offset);
        let last = span.bytes.end.saturating_sub(1).max(span.bytes.start);
        Some((line(span.bytes.start), line(last)))
    }
}

#[cfg(test)]
mod tests;
