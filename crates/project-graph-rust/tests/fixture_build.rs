//! Builds the driver with the pinned toolchain and graphs the fixture project
//! in `tests/fixture`. Needs the toolchain's `rustc-dev` component.

use project_graph::{
    BuildSelection, Dispatch, Endpoint, Fact, FactKind, ItemId, ModuleId, ProjectGraph,
    ProjectGrapher,
};
use project_graph_rust::{PINNED_TOOLCHAIN, RustGrapher, ToolchainCargo, build_driver};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture")
}

fn work() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("project-graph-rust")
}

/// The fixture's `app` binary, graphed once for all tests.
fn graph() -> &'static ProjectGraph {
    static GRAPH: OnceLock<ProjectGraph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let driver = build_driver(PINNED_TOOLCHAIN, &work().join("driver")).expect("driver builds");
        let grapher = RustGrapher::new(ToolchainCargo {
            toolchain: PINNED_TOOLCHAIN.into(),
            driver,
            work_dir: work().join("app"),
        });
        grapher
            .build_graph(&BuildSelection {
                root: fixture(),
                target: "app/bin:app".into(),
                configuration: "default features".into(),
            })
            .expect("the fixture graphs")
    })
}

fn item(id: &str) -> Endpoint {
    Endpoint::Item(ItemId(id.into()))
}

fn find<'a>(subject: &Endpoint, kind: &FactKind, object: &Endpoint) -> Vec<&'a Fact> {
    graph()
        .facts
        .values()
        .filter(|f| &f.subject == subject && &f.kind == kind && &f.object == object)
        .collect()
}

fn text(fact: &Fact) -> String {
    let site = fact.site.as_ref().expect("a source site");
    let source = std::fs::read_to_string(fixture().join(&site.file)).unwrap();
    source[site.bytes.clone()].to_string()
}

#[test]
fn every_package_in_the_build_is_a_module_with_its_unit() {
    let graph = graph();
    assert_eq!(
        graph
            .modules
            .keys()
            .map(|m| m.0.as_str())
            .collect::<Vec<_>>(),
        ["app", "consumer", "supplier"]
    );
    assert_eq!(
        graph.units.keys().map(|u| u.0.as_str()).collect::<Vec<_>>(),
        [
            "app/bin:app",
            "consumer/lib:consumer",
            "supplier/lib:supplier"
        ]
    );
    assert!(graph.gaps.is_empty(), "{:?}", graph.gaps);
}

#[test]
fn a_trait_object_call_stays_at_the_contract_and_a_direct_call_names_the_implementation() {
    let through_object = find(
        &item("consumer/lib:consumer::consume"),
        &FactKind::Call(Dispatch::Contract),
        &item("supplier/lib:supplier::Backups::write"),
    );
    assert_eq!(through_object.len(), 1);
    assert_eq!(text(through_object[0]), "parts.backups.write()");
    let direct = find(
        &item("app/bin:app::main"),
        &FactKind::Call(Dispatch::SelectedImplementation),
        &item("supplier/lib:supplier::{impl#0}::write"),
    );
    assert_eq!(direct.len(), 1);
    assert_eq!(text(direct[0]), "OsBackups.write()");
}

#[test]
fn a_call_inside_a_closure_belongs_to_the_enclosing_function() {
    let calls = find(
        &item("app/bin:app::main"),
        &FactKind::Call(Dispatch::Direct),
        &item("consumer/lib:consumer::consume"),
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(text(calls[0]), "consume(p)");
}

#[test]
fn a_third_party_crate_is_external_and_the_standard_library_is_left_out() {
    let graph = graph();
    let clock = find(
        &item("consumer/lib:consumer::consume"),
        &FactKind::Call(Dispatch::Direct),
        &Endpoint::External("clock::now".into()),
    );
    assert_eq!(clock.len(), 1);
    let mentions_std = graph.facts.values().any(|f| {
        [&f.subject, &f.object]
            .iter()
            .any(|e| matches!(e, Endpoint::External(name) if name.starts_with("std::") || name.starts_with("core::") || name.starts_with("alloc::")))
    });
    assert!(!mentions_std);
}

#[test]
fn a_renamed_dependency_is_declared_under_its_package_and_reexports_are_told_apart() {
    let consumer = Endpoint::Module(ModuleId("consumer".into()));
    assert_eq!(
        find(
            &consumer,
            &FactKind::DeclaredDependency,
            &Endpoint::Module(ModuleId("supplier".into()))
        )
        .len(),
        1
    );
    let reexport = find(
        &item("consumer/lib:consumer"),
        &FactKind::Reexport {
            exported_name: "reexported_helper".into(),
        },
        &item("supplier/lib:supplier::helper"),
    );
    assert_eq!(
        text(reexport[0]),
        "pub use store::helper as reexported_helper;"
    );
}

#[test]
fn an_implementation_in_the_same_package_as_its_trait_is_not_a_fact() {
    assert!(
        !graph()
            .facts
            .values()
            .any(|f| f.kind == FactKind::Implements)
    );
}

#[test]
fn the_implementation_behind_a_trait_object_call_is_traced_back_to_where_it_was_supplied() {
    let graph = graph();
    let call = find(
        &item("consumer/lib:consumer::consume"),
        &FactKind::Call(Dispatch::Contract),
        &item("supplier/lib:supplier::Backups::write"),
    )[0];
    let call_id = graph
        .facts
        .iter()
        .find(|(_, f)| *f == call)
        .map(|(id, _)| id.clone())
        .unwrap();
    let routes: Vec<_> = graph
        .routes
        .values()
        .filter(|r| r.use_site.as_ref() == Some(&call_id) && r.hops.len() > 1)
        .collect();
    assert_eq!(routes.len(), 1, "{routes:#?}");
    let route = routes[0];
    assert_eq!(route.origin, item("supplier/lib:supplier::OsBackups"));
    assert_eq!(route.end, project_graph::RouteEnd::ReachedUse);
    let kinds: Vec<_> = route
        .hops
        .iter()
        .map(|h| &graph.facts[&h.fact].kind)
        .collect();
    assert_eq!(
        kinds,
        [&FactKind::Coerce, &FactKind::Store, &FactKind::Read]
    );
    let coerce = &graph.facts[&route.hops[0].fact];
    assert_eq!(text(coerce), "Box::new(OsBackups)");
    assert_eq!(
        route.hops[1].to,
        Endpoint::Place {
            owner: ItemId("consumer/lib:consumer::Parts".into()),
            projection: "backups".into(),
        }
    );
    assert!(matches!(
        route.hops[1].support,
        project_graph::Support::Possible { .. }
    ));
}

#[test]
fn a_pub_use_other_crates_cannot_reach_is_an_import() {
    let import = find(
        &item("consumer/lib:consumer::inner"),
        &FactKind::Import {
            local_name: "inner_helper".into(),
            usage: project_graph::ImportUse::Undetermined,
        },
        &item("supplier/lib:supplier::helper"),
    );
    assert_eq!(import.len(), 1);
    assert!(!graph().facts.values().any(|f| matches!(
        &f.kind,
        FactKind::Reexport { exported_name } if exported_name == "inner_helper"
    )));
}

#[test]
fn a_trait_function_without_self_has_no_receiver_to_trace() {
    let graph = graph();
    let build = item("supplier/lib:supplier::Build::build");
    let calls = find(
        &item("consumer/lib:consumer::make"),
        &FactKind::Call(Dispatch::Contract),
        &build,
    );
    assert_eq!(calls.len(), 1, "the call itself is a fact");
    let traced = graph.routes.values().filter(|r| {
        r.use_site
            .as_ref()
            .is_some_and(|id| graph.facts[id].object == build && r.hops.len() != 1)
    });
    assert_eq!(traced.count(), 0, "its argument is not a receiver");
}

#[test]
fn an_outside_name_reached_through_a_local_alias_starts_with_its_package() {
    let externals: Vec<&String> = graph()
        .facts
        .values()
        .flat_map(|f| [&f.subject, &f.object])
        .filter_map(|e| match e {
            Endpoint::External(name) => Some(name),
            _ => None,
        })
        .collect();
    assert!(externals.iter().any(|n| n.as_str() == "clock::now"));
    let stray: Vec<_> = externals
        .iter()
        .filter(|n| !n.starts_with("clock"))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

#[test]
fn an_implementation_passed_into_an_async_fn_is_traced_through_its_coroutine() {
    let graph = graph();
    let call = find(
        &item("consumer/lib:consumer::write_later"),
        &FactKind::Call(Dispatch::Contract),
        &item("supplier/lib:supplier::Backups::write"),
    );
    assert_eq!(call.len(), 1);
    let call_id = graph
        .facts
        .iter()
        .find(|(_, f)| *f == call[0])
        .map(|(id, _)| id.clone())
        .unwrap();
    let reached: Vec<_> = graph
        .routes
        .values()
        .filter(|r| {
            r.use_site.as_ref() == Some(&call_id) && r.end == project_graph::RouteEnd::ReachedUse
        })
        .filter(|r| r.origin == item("supplier/lib:supplier::OsBackups"))
        .collect();
    assert_eq!(
        reached.len(),
        1,
        "{:#?}",
        graph
            .routes
            .values()
            .filter(|r| r.use_site.as_ref() == Some(&call_id))
            .collect::<Vec<_>>()
    );
    let kinds: Vec<_> = reached[0]
        .hops
        .iter()
        .map(|h| &graph.facts[&h.fact].kind)
        .collect();
    assert!(kinds.contains(&&FactKind::Pass), "{kinds:?}");
    assert!(kinds.contains(&&FactKind::Capture), "{kinds:?}");
}

#[test]
fn a_value_passed_through_the_standard_library_is_followed_as_a_possible_route() {
    let graph = graph();
    let call = find(
        &item("consumer/lib:consumer::write_some"),
        &FactKind::Call(Dispatch::Contract),
        &item("supplier/lib:supplier::Backups::write"),
    );
    assert_eq!(call.len(), 1);
    let call_id = graph
        .facts
        .iter()
        .find(|(_, f)| *f == call[0])
        .map(|(id, _)| id.clone())
        .unwrap();
    let reached: Vec<_> = graph
        .routes
        .values()
        .filter(|r| r.use_site.as_ref() == Some(&call_id))
        .filter(|r| r.origin == item("supplier/lib:supplier::OsBackups"))
        .collect();
    assert_eq!(reached.len(), 1, "{reached:#?}");
    assert_eq!(reached[0].end, project_graph::RouteEnd::ReachedUse);
    let last = reached[0].hops.last().unwrap();
    assert!(
        matches!(last.support, project_graph::Support::Possible { .. }),
        "Option::unwrap is assumed, not read: {:?}",
        last.support
    );
}

#[test]
fn a_unit_struct_in_an_option_reaches_a_generic_receiver_through_unwrap() {
    let graph = graph();
    let call = find(
        &item("consumer/lib:consumer::write_generic"),
        &FactKind::Call(Dispatch::Contract),
        &item("supplier/lib:supplier::Backups::write"),
    );
    assert_eq!(call.len(), 1);
    let call_id = graph
        .facts
        .iter()
        .find(|(_, f)| *f == call[0])
        .map(|(id, _)| id.clone())
        .unwrap();
    let reached: Vec<_> = graph
        .routes
        .values()
        .filter(|r| r.use_site.as_ref() == Some(&call_id))
        .filter(|r| r.origin == item("supplier/lib:supplier::OsBackups"))
        .collect();
    assert_eq!(reached.len(), 1, "{reached:#?}");
    assert_eq!(reached[0].end, project_graph::RouteEnd::ReachedUse);
}

#[test]
fn a_type_from_an_indirect_dependency_is_named_through_the_declared_one() {
    let names: Vec<&String> = graph()
        .facts
        .values()
        .filter_map(|f| match &f.object {
            Endpoint::External(name) if name.contains("Tick") => Some(name),
            _ => None,
        })
        .collect();
    assert!(!names.is_empty());
    assert!(
        names.iter().all(|n| n.as_str() == "clock::tick::Tick"),
        "{names:?}"
    );
}
