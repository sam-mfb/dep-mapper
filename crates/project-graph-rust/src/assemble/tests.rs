use super::*;
use crate::metadata::{Dependency, Member};

const SUPPLIER: &str = "s1";
const CONSUMER: &str = "c1";
const STD: &str = "std1";
const CLOCK: &str = "k1";

fn def(crate_name: &str, crate_id: &str, path: &str) -> r::Def {
    r::Def {
        crate_name: crate_name.into(),
        crate_id: crate_id.into(),
        path: path.into(),
        name: format!("{crate_name}{path}"),
    }
}

fn span(lo: usize) -> r::Span {
    r::Span {
        file: "/work/consumer/src/lib.rs".into(),
        lo,
        hi: lo + 4,
    }
}

fn unit(package: &str, id: &str, externs: &[&str]) -> r::Record {
    r::Record::Unit(r::UnitRecord {
        package: package.into(),
        crate_name: package.into(),
        crate_id: id.into(),
        crate_types: vec!["lib".into()],
        test: false,
        features: vec!["default".into()],
        target_triple: "aarch64-unknown-linux-gnu".into(),
        externs: externs.iter().map(|e| e.to_string()).collect(),
    })
}

fn item(crate_name: &str, crate_id: &str, path: &str, kind: r::ItemKind) -> r::Record {
    r::Record::Item(r::ItemRecord {
        def: def(crate_name, crate_id, path),
        kind,
        exported: true,
        signature: None,
        span: Some(span(0)),
    })
}

fn call(owner: r::Def, callee: r::Def, dispatch: r::Dispatch, lo: usize) -> r::Record {
    r::Record::Call(r::CallRecord {
        owner,
        callee: Some(callee),
        dispatch,
        span: span(lo),
    })
}

fn workspace() -> Workspace {
    let member = |name: &str, deps: &[(&str, &str, bool)]| Member {
        name: name.into(),
        root: name.into(),
        dependencies: deps
            .iter()
            .map(|(as_named, package, member)| {
                (
                    as_named.to_string(),
                    Dependency {
                        package: package.to_string(),
                        member: *member,
                    },
                )
            })
            .collect(),
    };
    Workspace {
        root: "/work".into(),
        members: vec![
            member("supplier", &[]),
            member(
                "consumer",
                &[
                    ("store", "supplier", true),
                    ("clock", "clock", false),
                    // Its tests use it with extra features.
                    ("consumer", "consumer", true),
                ],
            ),
        ],
        third_party: [("clock".to_string(), "clock".to_string())].into(),
    }
}

fn crates() -> Vec<Vec<r::Record>> {
    let consume = def("consumer", CONSUMER, "::consume");
    vec![
        vec![
            unit("supplier", SUPPLIER, &[]),
            item("supplier", SUPPLIER, "::Backups", r::ItemKind::Type),
            item(
                "supplier",
                SUPPLIER,
                "::Backups::write",
                r::ItemKind::Method,
            ),
            item("supplier", SUPPLIER, "::helper", r::ItemKind::Function),
        ],
        vec![
            unit("consumer", CONSUMER, &["store", "clock", "consumer"]),
            item("consumer", CONSUMER, "", r::ItemKind::Module),
            item("consumer", CONSUMER, "::consume", r::ItemKind::Function),
            item("consumer", CONSUMER, "::local", r::ItemKind::Function),
            call(
                consume.clone(),
                def("supplier", SUPPLIER, "::Backups::write"),
                r::Dispatch::Contract,
                10,
            ),
            call(
                consume.clone(),
                def("clock", CLOCK, "::now"),
                r::Dispatch::Direct,
                20,
            ),
            call(
                consume.clone(),
                def("std", STD, "::io::_print"),
                r::Dispatch::Direct,
                30,
            ),
            call(
                def("consumer", CONSUMER, "::consume::{closure#0}"),
                def("consumer", CONSUMER, "::local"),
                r::Dispatch::Direct,
                40,
            ),
            // The same call written twice by a macro expansion.
            call(
                consume,
                def("supplier", SUPPLIER, "::Backups::write"),
                r::Dispatch::Contract,
                10,
            ),
            r::Record::Use(r::UseRecord {
                module: def("consumer", CONSUMER, ""),
                target: def("supplier", SUPPLIER, "::helper"),
                local_name: "reexported_helper".into(),
                public: true,
                span: span(50),
            }),
            r::Record::Use(r::UseRecord {
                module: def("consumer", CONSUMER, ""),
                target: def("supplier", SUPPLIER, "::Backups"),
                local_name: "Backups".into(),
                public: false,
                span: span(60),
            }),
            r::Record::Gap(r::GapRecord {
                span: Some(span(70)),
                reason: "no body".into(),
            }),
        ],
    ]
}

fn snapshot() -> Snapshot {
    Snapshot {
        project: "consumer".into(),
        source_revision: "abc".into(),
        target: "consumer".into(),
        configuration: "default features".into(),
    }
}

fn built() -> ProjectGraph {
    graph(&workspace(), &crates(), snapshot())
}

fn item_id(id: &str) -> Endpoint {
    Endpoint::Item(ItemId(id.into()))
}

fn facts_of(graph: &ProjectGraph, kind: fn(&FactKind) -> bool) -> Vec<&Fact> {
    graph.facts.values().filter(|f| kind(&f.kind)).collect()
}

#[test]
fn packages_are_modules_and_compiled_crates_are_units() {
    let graph = built();
    assert_eq!(
        graph
            .modules
            .keys()
            .map(|m| m.0.as_str())
            .collect::<Vec<_>>(),
        ["consumer", "supplier"]
    );
    let unit = &graph.units[&UnitId("consumer/lib:consumer".into())];
    assert_eq!(unit.module, ModuleId("consumer".into()));
    assert_eq!(unit.settings["features"], "default");
    assert_eq!(graph.build.source_revision, "abc");
    assert!(
        graph
            .items
            .contains_key(&ItemId("supplier/lib:supplier::Backups::write".into()))
    );
}

#[test]
fn calls_into_other_modules_and_third_party_crates_are_kept_and_std_is_left_out() {
    let graph = built();
    let calls = facts_of(&graph, |k| matches!(k, FactKind::Call(_)));
    let described: Vec<_> = calls
        .iter()
        .map(|f| (&f.subject, &f.kind, &f.object))
        .collect();
    assert_eq!(
        described,
        [
            (
                &item_id("consumer/lib:consumer::consume"),
                &FactKind::Call(Dispatch::Contract),
                &item_id("supplier/lib:supplier::Backups::write")
            ),
            (
                &item_id("consumer/lib:consumer::consume"),
                &FactKind::Call(Dispatch::Direct),
                &Endpoint::External("clock::now".into())
            ),
        ],
        "the duplicate, the call within consumer and the std call are not facts"
    );
    assert_eq!(calls[0].site.as_ref().unwrap().file, "consumer/src/lib.rs");
}

#[test]
fn a_public_use_is_a_reexport_and_a_private_use_is_an_import_of_undetermined_use() {
    let graph = built();
    let reexports = facts_of(&graph, |k| matches!(k, FactKind::Reexport { .. }));
    assert_eq!(reexports.len(), 1);
    assert_eq!(reexports[0].subject, item_id("consumer/lib:consumer"));
    assert_eq!(
        reexports[0].object,
        item_id("supplier/lib:supplier::helper")
    );
    let imports = facts_of(&graph, |k| matches!(k, FactKind::Import { .. }));
    assert_eq!(
        imports[0].kind,
        FactKind::Import {
            local_name: "Backups".into(),
            usage: ImportUse::Undetermined
        }
    );
}

#[test]
fn declared_dependencies_name_the_package_behind_a_renamed_dependency_and_skip_the_package_itself()
{
    let graph = built();
    let declared: BTreeSet<_> = facts_of(&graph, |k| matches!(k, FactKind::DeclaredDependency))
        .into_iter()
        .map(|f| format!("{:?}", f.object))
        .collect();
    let expected: BTreeSet<_> = [
        Endpoint::Module(ModuleId("supplier".into())),
        Endpoint::External("clock".into()),
    ]
    .iter()
    .map(|e| format!("{e:?}"))
    .collect();
    assert_eq!(declared, expected);
}

#[test]
fn every_fact_is_a_route_of_one_hop_from_what_is_used_to_its_user() {
    let graph = built();
    assert_eq!(graph.routes.len(), graph.facts.len());
    for route in graph.routes.values() {
        let fact = &graph.facts[route.use_site.as_ref().unwrap()];
        assert_eq!(route.origin, fact.object);
        assert_eq!(route.hops.len(), 1);
        assert_eq!(route.hops[0].to, fact.subject);
        assert_eq!(route.end, RouteEnd::ReachedUse);
    }
}

#[test]
fn gaps_keep_their_unit_and_site() {
    let graph = built();
    assert_eq!(graph.gaps.len(), 1);
    assert_eq!(
        graph.gaps[0].unit,
        Some(UnitId("consumer/lib:consumer".into()))
    );
    assert_eq!(graph.gaps[0].site.as_ref().unwrap().bytes, 70..74);
}

#[test]
fn the_same_records_in_another_order_give_the_same_graph() {
    let mut reversed = crates();
    reversed.reverse();
    for records in &mut reversed {
        records[1..].reverse();
    }
    assert_eq!(graph(&workspace(), &reversed, snapshot()), built());
}
