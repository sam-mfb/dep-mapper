//! Adds what the model needs beyond a shared piece's text. A module piece
//! lists its routes in one line each; this adds each route's hops. Every
//! piece then gets source excerpts for the facts and items it mentions, so
//! the model reads the code itself, not only a description of it.

use crate::source::SourceText;
use graph_evaluator_pieces::handles::Handle;
use graph_evaluator_pieces::workbook::{Piece, PieceSubject, Workbook};
use project_graph::{ProjectGraph, SourceSpan};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most excerpts added to one page.
    pub excerpts_per_page: usize,
}

/// A page ready for the model, and any source it could not show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Supplemented {
    pub text: String,
    pub source_problems: Vec<String>,
}

/// The handles the text mentions that the workbook knows, in order of first
/// mention.
fn mentioned(text: &str, workbook: &Workbook) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut found = vec![];
    for word in text.split_whitespace() {
        let mut word = word.trim_start_matches(['(', '[', '`']);
        // Strip trailing punctuation until the rest is a known handle.
        loop {
            if workbook.refs.contains_key(word) {
                if seen.insert(word.to_string()) {
                    found.push(word.to_string());
                }
                break;
            }
            match word.char_indices().last() {
                Some((index, ',' | '.' | ';' | ':' | ')' | ']' | '`')) => {
                    word = &word[..index];
                }
                _ => break,
            }
        }
    }
    found
}

fn span_of(graph: &ProjectGraph, handle: &str) -> Option<SourceSpan> {
    match Handle::parse(handle).ok()? {
        Handle::Fact(id) => graph.facts.get(&id)?.site.clone(),
        Handle::Item(id) => graph.items.get(&id)?.declaration.clone(),
        Handle::Route(_) | Handle::Module(_) => None,
    }
}

pub fn supplement(
    graph: &ProjectGraph,
    workbook: &Workbook,
    piece: &Piece,
    page: &str,
    source: &dyn SourceText,
    limits: &Limits,
) -> Supplemented {
    let mut text = page.trim_end().to_string();
    if let PieceSubject::Module { .. } = piece.subject {
        let routes: Vec<String> = mentioned(page, workbook)
            .into_iter()
            .filter(|handle| handle.starts_with("route:"))
            .collect();
        if !routes.is_empty() {
            text.push_str("\n\nRoute details:");
            for route in routes {
                let _ = write!(text, "\n{}", workbook.refs[&route].detail);
            }
        }
    }
    let mut problems = vec![];
    let mut excerpts = String::new();
    let mut shown = 0;
    let mut omitted = 0;
    for handle in mentioned(&text, workbook) {
        let Some(span) = span_of(graph, &handle) else {
            continue;
        };
        if shown == limits.excerpts_per_page {
            omitted += 1;
            continue;
        }
        match source.excerpt(&span) {
            Ok(excerpt) => {
                shown += 1;
                let _ = write!(
                    excerpts,
                    "\n{handle} at {}:{}-{}",
                    span.file, excerpt.first_line, excerpt.last_line
                );
                for line in excerpt.text.lines() {
                    let _ = write!(excerpts, "\n| {line}");
                }
                if excerpt.cut {
                    excerpts.push_str("\n| ...");
                }
            }
            Err(error) => problems.push(format!("{handle}: {}", error.message)),
        }
    }
    if !excerpts.is_empty() {
        text.push_str("\n\nSource excerpts:");
        text.push_str(&excerpts);
    }
    if omitted > 0 {
        let _ = write!(
            text,
            "\n\n{omitted} more excerpts were left out to keep the page short."
        );
    }
    text.push('\n');
    Supplemented {
        text,
        source_problems: problems,
    }
}

#[cfg(test)]
mod tests;
