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
