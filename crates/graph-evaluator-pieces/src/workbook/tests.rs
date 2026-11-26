use super::*;
use crate::testing;
use project_graph::{FactId, RouteEnd};

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
    let mut graph = testing::graph();
    let mut second = graph.items[&project_graph::ItemId("app::main".into())].clone();
    second.name = "helper".into();
    graph
        .items
        .insert(project_graph::ItemId("app::helper".into()), second);
    let workbook = present(
        &graph,
        &Presentation {
            page_size: 1,
            source: &ByteOffsets,
            command: Some("srs eval"),
        },
    );
    let pages = &workbook.piece(&PieceId::module("app")).unwrap().pages;
    assert_eq!(pages.len(), 2);
    assert!(pages[0].contains("(page 1 of 2)") && pages[1].contains("(page 2 of 2)"));
    assert!(pages[0].contains("item:app::helper") && pages[1].contains("item:app::main"));
    assert!(pages.iter().all(|p| {
        p.contains("Routes that start, pass through or end in this module: 2")
            && p.contains("srs eval defer")
    }));
}

#[test]
fn a_module_piece_counts_its_routes_by_where_they_lead_and_names_the_piece_that_lists_them() {
    let workbook = workbook(10);
    let app = &workbook.piece(&PieceId::module("app")).unwrap().pages[0];
    assert!(app.contains("- 1 with disk, listed in routes/app+disk; for example route:r-wire"));
    assert!(app.contains("- 1 leaving for std, listed in routes/app; for example route:r-write"));
    let disk = &workbook.piece(&PieceId::module("disk")).unwrap().pages[0];
    assert!(disk.contains("with store, listed in routes/disk+store"));
    assert!(disk.contains("with app, listed in routes/app+disk"));
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

#[test]
fn every_module_piece_shows_the_routes_left_out_of_the_whole_graph() {
    let mut graph = testing::graph();
    route_exclusion::exclude(&mut graph, &["std".to_string()], |name| {
        name.starts_with("std::")
    });
    let workbook = present(
        &graph,
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: Some("srs eval"),
        },
    );
    for module in ["app", "disk", "store"] {
        let page = &workbook.piece(&PieceId::module(module)).unwrap().pages[0];
        assert!(
            page.contains(
                "Coverage gaps for the whole graph:\n- Routes that start in the excluded packages std are left out: 0 routes and 0 facts removed."
            ),
            "{page}"
        );
    }
    let app = &workbook.piece(&PieceId::module("app")).unwrap().pages[0];
    assert!(
        app.contains("Coverage gaps:\n- macro not expanded"),
        "{app}"
    );
}

#[test]
fn a_route_used_at_many_sites_shows_the_first_few_and_counts_the_rest() {
    let mut graph = testing::graph();
    let write = graph.facts[&FactId("f-write".into())].clone();
    let sites: Vec<FactId> = (1..=5).map(|n| FactId(format!("f-write-{n}"))).collect();
    for (n, id) in sites.iter().enumerate() {
        let mut fact = write.clone();
        fact.site.as_mut().unwrap().bytes = n * 10..n * 10 + 5;
        graph.facts.insert(id.clone(), fact);
    }
    let route = graph.routes.get_mut(&RouteId("r-write".into())).unwrap();
    route.use_sites = sites.clone();
    route.hops[0].facts = sites.clone();
    route.end = RouteEnd::Stopped {
        at: sites,
        reason: "no origin".into(),
    };
    let workbook = present(
        &graph,
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: Some("srs eval"),
        },
    );
    let info = &workbook.refs["route:r-write"];
    // The handle's own detail names every site of every list.
    let detail = &info.detail;
    assert!(
        detail.contains("explains the uses at 5 sites:\n    fact:f-write-1 ("),
        "{detail}"
    );
    for n in 1..=5 {
        assert_eq!(
            detail.matches(&format!("fact:f-write-{n} (")).count(),
            3,
            "site {n} is a use, the hop's fact and a stop: {detail}"
        );
    }
    assert!(detail.contains("stops: no origin; at\n"), "{detail}");
    // Pieces show the summary, which names the first few and points to the
    // handle for the rest.
    let summary = info.summary.as_deref().unwrap();
    assert_eq!(info.brief_detail(), summary);
    assert!(
        summary.contains("explains the uses at 5 sites: fact:f-write-1 ("),
        "{summary}"
    );
    assert_eq!(
        summary
            .matches("and 2 more, which `srs eval show route:r-write` lists")
            .count(),
        3,
        "{summary}"
    );
    assert!(!summary.contains("fact:f-write-4"), "{summary}");
    let piece = &workbook.piece_of_route("route:r-write").unwrap().pages[0];
    assert!(piece.contains(summary), "{piece}");
    // A route with one fact per list needs no summary.
    assert_eq!(workbook.refs["route:r-wire"].summary, None);
    for n in 1..=5 {
        assert!(workbook.refs.contains_key(&format!("fact:f-write-{n}")));
    }
}

#[test]
fn without_a_command_a_summary_counts_the_rest_without_naming_a_command() {
    let mut graph = testing::graph();
    let write = graph.facts[&FactId("f-write".into())].clone();
    let sites: Vec<FactId> = (1..=4).map(|n| FactId(format!("f-write-{n}"))).collect();
    for id in &sites {
        graph.facts.insert(id.clone(), write.clone());
    }
    graph
        .routes
        .get_mut(&RouteId("r-write".into()))
        .unwrap()
        .use_sites = sites;
    let workbook = present(
        &graph,
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: None,
        },
    );
    let summary = workbook.refs["route:r-write"].brief_detail();
    assert!(summary.contains(") and 1 more\n"), "{summary}");
    assert!(!summary.contains("show"), "{summary}");
}
