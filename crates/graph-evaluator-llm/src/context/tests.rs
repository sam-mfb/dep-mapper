use super::*;
use crate::source::{Excerpt, LinesFrom, NoSource, locate};
use graph_evaluation_assembly::testing;
use graph_evaluation_assembly::workbook::{ClusterPlan, Presentation, present};

/// Every file has the same five ten-byte lines.
pub(crate) struct SameText;

pub(crate) const TEXT: &str = "line one.\nline two.\nline 333.\nline 444.\nline 555.\n";

impl SourceText for SameText {
    fn excerpt(&self, span: &SourceSpan) -> Result<Excerpt, crate::source::SourceError> {
        locate(TEXT, &span.bytes, 2)
    }
}

fn workbook(graph: &ProjectGraph) -> Workbook {
    present(
        graph,
        &Presentation {
            page_size: 40,
            source: &LinesFrom(&SameText),
            command: None,
            clusters: &ClusterPlan::proposed(),
        },
    )
}

fn piece<'a>(workbook: &'a Workbook, id: &str) -> &'a Piece {
    workbook
        .pieces
        .iter()
        .find(|piece| piece.id.0 == id)
        .unwrap()
}

fn limits(excerpts_per_page: usize) -> Limits {
    Limits { excerpts_per_page }
}

#[test]
fn a_module_piece_gains_route_details_and_excerpts_for_what_they_mention() {
    let graph = testing::graph();
    let workbook = workbook(&graph);
    let piece = piece(&workbook, "module/app");
    let result = supplement(
        &graph,
        &workbook,
        piece,
        &piece.pages[0],
        &SameText,
        &limits(10),
    );
    let text = &result.text;
    assert!(text.contains("Route details:"), "{text}");
    assert!(text.contains("fact:f-construct"), "{text}");
    assert!(
        text.contains("\nitem:app::main at crates/app/src/lib.rs:2-2\n| line two."),
        "{text}"
    );
    assert!(
        text.contains("\nfact:f-write at crates/app/src/lib.rs:4-4\n| line 444."),
        "{text}"
    );
    assert!(result.source_problems.is_empty());
}

#[test]
fn a_routes_piece_gains_excerpts_but_no_route_details() {
    let graph = testing::graph();
    let workbook = workbook(&graph);
    let piece = piece(&workbook, "routes/disk+store");
    let text = supplement(
        &graph,
        &workbook,
        piece,
        &piece.pages[0],
        &SameText,
        &limits(10),
    )
    .text;
    assert!(!text.contains("Route details:"), "{text}");
    assert!(
        text.contains("fact:f-impl at crates/disk/src/lib.rs:4-4"),
        "{text}"
    );
}

#[test]
fn excerpts_past_the_limit_are_counted_not_shown() {
    let graph = testing::graph();
    let workbook = workbook(&graph);
    let piece = piece(&workbook, "module/app");
    let text = supplement(
        &graph,
        &workbook,
        piece,
        &piece.pages[0],
        &SameText,
        &limits(1),
    )
    .text;
    assert_eq!(text.matches("\n| ").count(), 1, "{text}");
    assert!(text.contains("more excerpts were left out"), "{text}");
}

#[test]
fn unreadable_source_is_reported_not_shown() {
    let graph = testing::graph();
    let workbook = workbook(&graph);
    let piece = piece(&workbook, "routes/disk+store");
    let result = supplement(
        &graph,
        &workbook,
        piece,
        &piece.pages[0],
        &NoSource,
        &limits(10),
    );
    assert!(!result.text.contains("Source excerpts:"));
    assert_eq!(
        result.source_problems,
        [
            "item:disk::DiskStore: source for crates/disk/src/lib.rs is not available",
            "item:store::Store: source for crates/store/src/lib.rs is not available",
            "fact:f-impl: source for crates/disk/src/lib.rs is not available",
        ]
    );
}

#[test]
fn handles_are_found_through_surrounding_punctuation() {
    let graph = testing::graph();
    let workbook = workbook(&graph);
    assert_eq!(
        mentioned("see (fact:f-impl), then `item:app::main`.", &workbook),
        vec!["fact:f-impl".to_string(), "item:app::main".to_string()]
    );
}
