//! Turns a `SourceSpan`, a file and a byte range, into line numbers and a
//! short excerpt for the model to read. `ProjectFiles` reads the project's
//! files from disk; `locate` does the arithmetic on text already read.

use graph_evaluator_pieces::workbook::SourceLines;
use project_graph::SourceSpan;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;

pub trait SourceText {
    fn excerpt(&self, span: &SourceSpan) -> Result<Excerpt, SourceError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excerpt {
    /// One-based and inclusive.
    pub first_line: usize,
    pub last_line: usize,
    /// The whole lines the span touches, cut to the excerpt limit. Empty when
    /// the limit is zero.
    pub text: String,
    /// Whether `text` stops before `last_line`.
    pub cut: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct SourceError {
    pub message: String,
}

/// The lines `bytes` touches in `source`, keeping at most `max_lines` of them.
pub fn locate(
    source: &str,
    bytes: &std::ops::Range<usize>,
    max_lines: usize,
) -> Result<Excerpt, SourceError> {
    if bytes.start > bytes.end
        || bytes.end > source.len()
        || !source.is_char_boundary(bytes.start)
        || !source.is_char_boundary(bytes.end)
    {
        return Err(SourceError {
            message: format!(
                "byte range {}..{} does not fit the {}-byte file",
                bytes.start,
                bytes.end,
                source.len()
            ),
        });
    }
    let line_of = |offset: usize| source[..offset].matches('\n').count() + 1;
    let first_line = line_of(bytes.start);
    // A span that ends with a newline ends on the line that newline closes.
    let end = if bytes.end > bytes.start && source.as_bytes()[bytes.end - 1] == b'\n' {
        bytes.end - 1
    } else {
        bytes.end
    };
    let last_line = line_of(end);
    let lines: Vec<&str> = source
        .lines()
        .skip(first_line - 1)
        .take(last_line - first_line + 1)
        .collect();
    let kept = lines.len().min(max_lines);
    Ok(Excerpt {
        first_line,
        last_line,
        text: lines[..kept].join("\n"),
        cut: kept < lines.len(),
    })
}

/// Reads files under a project root, each at most once.
pub struct ProjectFiles {
    root: PathBuf,
    max_lines: usize,
    files: Mutex<HashMap<String, Result<String, String>>>,
}

impl ProjectFiles {
    /// Excerpts keep at most `max_lines` lines.
    pub fn new(root: PathBuf, max_lines: usize) -> Self {
        ProjectFiles {
            root,
            max_lines,
            files: Mutex::new(HashMap::new()),
        }
    }
}

impl SourceText for ProjectFiles {
    fn excerpt(&self, span: &SourceSpan) -> Result<Excerpt, SourceError> {
        let mut files = self.files.lock().map_err(|_| SourceError {
            message: "the source cache was poisoned by an earlier panic".into(),
        })?;
        let text = files.entry(span.file.clone()).or_insert_with(|| {
            std::fs::read_to_string(self.root.join(&span.file)).map_err(|error| error.to_string())
        });
        match text {
            Ok(text) => locate(text, &span.bytes, self.max_lines),
            Err(error) => Err(SourceError {
                message: format!("cannot read {}: {error}", span.file),
            }),
        }
    }
}

/// Gives no source text. Use it when the project's files are not available.
pub struct NoSource;

impl SourceText for NoSource {
    fn excerpt(&self, span: &SourceSpan) -> Result<Excerpt, SourceError> {
        Err(SourceError {
            message: format!("source for {} is not available", span.file),
        })
    }
}

/// Line numbers for the shared piece text, taken from a `SourceText`. A span
/// whose source cannot be read is shown by byte offsets; the excerpts added
/// to the piece report why.
pub struct LinesFrom<'a>(pub &'a dyn SourceText);

impl SourceLines for LinesFrom<'_> {
    fn lines(&self, span: &SourceSpan) -> Option<(usize, usize)> {
        self.0
            .excerpt(span)
            .ok()
            .map(|excerpt| (excerpt.first_line, excerpt.last_line))
    }
}

#[cfg(test)]
mod tests;
