use super::*;
use crate::testing;

struct EveryTenBytes;

impl SourceLines for EveryTenBytes {
    fn lines(&self, span: &SourceSpan) -> Option<(usize, usize)> {
        Some((
            span.bytes.start / 10 + 1,
            span.bytes.end.saturating_sub(1) / 10 + 1,
        ))
    }
}

fn workbook(page_size: usize) -> Workbook {
    present(
        &testing::graph(),
        &Presentation {
            page_size,
            source: &ByteOffsets,
            command: Some("srs eval"),
        },
    )
}

fn ids(workbook: &Workbook) -> Vec<&str> {
    workbook.pieces.iter().map(|p| p.id.0.as_str()).collect()
}

#[test]
fn pieces_are_modules_then_clusters_then_routes_grouped_by_their_end_modules() {
    let workbook = workbook(10);
    assert_eq!(
        ids(&workbook),
        [
            "module/app",
            "module/disk",
            "module/store",
            "clusters",
            "routes/app",
            "routes/app+disk",
            "routes/disk+store"
        ]
    );
    assert_eq!(
        workbook.piece_of_route("route:r-write").unwrap().id.0,
        "routes/app"
    );
    assert_eq!(
        workbook.piece_of_route("route:r-impl").unwrap().id.0,
        "routes/disk+store"
    );
    assert_eq!(workbook.graph, "fixture@1");
}

#[test]
fn every_route_fact_and_item_is_indexed_with_the_modules_it_touches_and_its_fact_kinds() {
    let workbook = workbook(10);
    assert_eq!(workbook.refs.len(), 3 + 3 + 3);
    let wire = &workbook.refs["route:r-wire"];
    assert_eq!(
        wire.modules,
        BTreeSet::from(["app".to_string(), "disk".to_string()])
    );
    assert_eq!(wire.fact_kinds, BTreeSet::from(["construct".to_string()]));
    let implements = &workbook.refs["fact:f-impl"];
    assert_eq!(
        implements.modules,
        BTreeSet::from(["disk".to_string(), "store".to_string()])
    );
    assert!(implements.detail.contains("implements"));
    assert!(!workbook.refs.contains_key("module:app"));
}

#[test]
fn a_module_piece_lists_its_items_gaps_and_routes_and_how_to_answer() {
    let workbook = workbook(10);
    let page = &workbook.piece(&PieceId::module("app")).unwrap().pages[0];
    assert!(page.contains("item:app::main"));
    assert!(page.contains("macro not expanded"));
    assert!(page.contains("route:r-wire"));
    assert!(page.contains("route:r-write"));
    assert!(!page.contains("route:r-impl"));
    assert!(page.contains("srs eval judge module:app"));
}

#[test]
fn long_lists_are_paged_and_each_page_repeats_the_heading_and_instructions() {
    let workbook = workbook(1);
    let pages = &workbook.piece(&PieceId::module("app")).unwrap().pages;
    assert_eq!(pages.len(), 2);
    assert!(pages[0].contains("(page 1 of 2)") && pages[1].contains("(page 2 of 2)"));
    assert!(
        pages
            .iter()
            .all(|p| p.contains("item:app::main") && p.contains("srs eval defer"))
    );
}

#[test]
fn spans_show_lines_when_the_source_is_available_and_byte_offsets_otherwise() {
    let with_lines = present(
        &testing::graph(),
        &Presentation {
            page_size: 10,
            source: &EveryTenBytes,
            command: Some("srs eval"),
        },
    );
    assert!(
        with_lines.refs["fact:f-impl"]
            .detail
            .contains("crates/disk/src/lib.rs:4")
    );
    assert!(
        workbook(10).refs["fact:f-impl"]
            .detail
            .contains("crates/disk/src/lib.rs bytes 30..40")
    );
}

#[test]
fn a_workbook_survives_being_written_out_and_read_back() {
    // The store writes workbooks as JSON; the shape must round-trip.
    let workbook = workbook(10);
    let text = serde_json::to_string(&workbook).unwrap();
    assert_eq!(serde_json::from_str::<Workbook>(&text).unwrap(), workbook);
}

#[test]
fn without_a_command_pieces_hold_the_evidence_and_no_command_lines() {
    let workbook = present(
        &testing::graph(),
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: None,
        },
    );
    for piece in &workbook.pieces {
        for page in &piece.pages {
            assert!(
                !page.contains("srs eval") && !page.contains("To record"),
                "{page}"
            );
        }
    }
    let module = &workbook.piece(&PieceId::module("app")).unwrap().pages[0];
    assert!(module.contains("item:app::main") && module.contains("route:r-wire"));
    let clusters = &workbook.piece(&PieceId::clusters()).unwrap().pages[0];
    assert!(clusters.contains("Every cluster a module judgment names must be listed."));
}
