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
        depends_on: BTreeMap::new(),
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

fn local(function: &str, local: u32) -> r::Node {
    r::Node::Local {
        function: def("consumer", CONSUMER, function),
        local,
        name: None,
    }
}

fn receiver(node: r::Node, lo: usize) -> r::Record {
    r::Record::Receiver(r::ReceiverRecord {
        owner: def("consumer", CONSUMER, "::consume"),
        contract: def("supplier", SUPPLIER, "::Backups::write"),
        contract_trait: def("supplier", SUPPLIER, "::Backups"),
        receiver: node,
        span: span(lo),
    })
}

fn flow(from: r::Node, to: r::Node, kind: r::FlowKind) -> r::Record {
    r::Record::Flow(r::FlowRecord {
        from,
        to,
        kind,
        call_site: None,
        span: Some(span(5)),
    })
}

/// consume receives an OsBackups from supplier and calls write on it; a
/// second call's receiver has no known origin; a third receives a type
/// defined in consumer itself.
fn crates_with_flows() -> Vec<Vec<r::Record>> {
    let mut crates = crates();
    crates[0].extend([
        item("supplier", SUPPLIER, "::OsBackups", r::ItemKind::Type),
        os_backups_implements(),
    ]);
    crates[1].extend([
        item("consumer", CONSUMER, "::Local", r::ItemKind::Type),
        // `Local` implements `Backups` in consumer itself.
        r::Record::Impl(r::ImplRecord {
            self_type: Some(def("consumer", CONSUMER, "::Local")),
            contract: def("supplier", SUPPLIER, "::Backups"),
            span: span(0),
        }),
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::OsBackups")),
            local("::consume", 2),
            r::FlowKind::Coerce,
        ),
        flow(
            local("::consume", 2),
            local("::consume", 3),
            r::FlowKind::Copy,
        ),
        receiver(local("::consume", 3), 10),
        receiver(local("::consume", 9), 80),
        flow(
            r::Node::Type(def("consumer", CONSUMER, "::Local")),
            local("::consume", 11),
            r::FlowKind::Coerce,
        ),
        receiver(local("::consume", 11), 90),
    ]);
    crates
}

fn traced_routes(graph: &ProjectGraph) -> Vec<&Route> {
    graph
        .routes
        .iter()
        .filter(|(id, _)| id.0.starts_with("route:traced:"))
        .map(|(_, r)| r)
        .collect()
}

#[test]
fn a_traced_route_runs_from_the_supplied_type_through_reported_moves_to_the_call() {
    let graph = graph(&workspace(), &crates_with_flows(), snapshot());
    let traced = traced_routes(&graph);
    let reached: Vec<_> = traced
        .iter()
        .filter(|r| r.end == RouteEnd::ReachedUse)
        .collect();
    assert_eq!(reached.len(), 1, "the same-module origin is not a route");
    let route = reached[0];
    assert_eq!(route.origin, item_id("supplier/lib:supplier::OsBackups"));
    assert_eq!(
        route.hops.len(),
        1,
        "the copy between temporaries is not a hop"
    );
    assert_eq!(graph.facts[&route.hops[0].fact].kind, FactKind::Coerce);
    assert_eq!(
        route.hops[0].to,
        Endpoint::Place {
            owner: ItemId("consumer/lib:consumer::consume".into()),
            projection: "_2".into(),
        }
    );
    let call = &graph.facts[route.use_site.as_ref().unwrap()];
    assert_eq!(call.kind, FactKind::Call(Dispatch::Contract));
    assert_eq!(
        call.site.as_ref().unwrap().bytes,
        10..14,
        "the same call fact as the one-hop route"
    );
}

#[test]
fn a_cross_module_call_with_no_origin_found_stops_at_the_call() {
    let graph = graph(&workspace(), &crates_with_flows(), snapshot());
    let stopped: Vec<_> = traced_routes(&graph)
        .into_iter()
        .filter(|r| matches!(r.end, RouteEnd::Stopped { .. }))
        .collect();
    assert_eq!(stopped.len(), 1);
    let RouteEnd::Stopped { at, reason } = &stopped[0].end else {
        unreachable!()
    };
    assert_eq!(stopped[0].use_site.as_ref(), Some(at));
    assert_eq!(graph.facts[at].site.as_ref().unwrap().bytes, 80..84);
    assert!(reason.contains("no flow"));
    assert!(stopped[0].hops.is_empty());
}

#[test]
fn shadowed_variables_with_one_name_stay_apart() {
    let named = |local| r::Node::Local {
        function: def("consumer", CONSUMER, "::consume"),
        local,
        name: Some("backups".into()),
    };
    let mut crates = crates();
    crates[0].extend([
        item("supplier", SUPPLIER, "::OsBackups", r::ItemKind::Type),
        os_backups_implements(),
    ]);
    crates[1].extend([
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::OsBackups")),
            named(2),
            r::FlowKind::Coerce,
        ),
        // A second `let backups` whose value nobody supplies.
        receiver(named(7), 80),
    ]);
    let graph = graph(&workspace(), &crates, snapshot());
    let traced = traced_routes(&graph);
    assert_eq!(traced.len(), 1);
    assert!(
        matches!(traced[0].end, RouteEnd::Stopped { .. }),
        "{traced:#?}"
    );
}

#[test]
fn outside_names_start_with_their_package() {
    let def = |name: &str| r::Def {
        crate_name: "tokio_util".into(),
        crate_id: "t1".into(),
        path: "::codec::Framed".into(),
        name: name.into(),
    };
    assert_eq!(
        external_name(None, "tokio-util", &def("tokio_util::codec::Framed")),
        "tokio-util::codec::Framed"
    );
    assert_eq!(
        external_name(
            Some("tokio-util"),
            "tokio-util",
            &def("<&T as tokio_util::Decode>::decode")
        ),
        "tokio-util::<&T as tokio_util::Decode>::decode"
    );
}

#[test]
fn a_hop_after_an_unreported_library_pass_is_possible() {
    let crates = {
        let mut crates = crates();
        crates[0].extend([
            item("supplier", SUPPLIER, "::OsBackups", r::ItemKind::Type),
            os_backups_implements(),
        ]);
        crates[1].extend([
            flow(
                r::Node::Type(def("supplier", SUPPLIER, "::OsBackups")),
                local("::consume", 2),
                r::FlowKind::Coerce,
            ),
            // `let b = maybe.unwrap();`
            flow(
                local("::consume", 2),
                local("::consume", 3),
                r::FlowKind::Through,
            ),
            receiver(local("::consume", 3), 10),
        ]);
        crates
    };
    let graph = graph(&workspace(), &crates, snapshot());
    let reached: Vec<_> = traced_routes(&graph)
        .into_iter()
        .filter(|r| r.end == RouteEnd::ReachedUse)
        .collect();
    assert_eq!(reached.len(), 1);
    assert_eq!(reached[0].hops.len(), 1, "the pass itself is not a hop");
    assert!(matches!(
        reached[0].hops[0].support,
        Support::Possible { .. }
    ));
}

fn os_backups_implements() -> r::Record {
    r::Record::Impl(r::ImplRecord {
        self_type: Some(def("supplier", SUPPLIER, "::OsBackups")),
        contract: def("supplier", SUPPLIER, "::Backups"),
        span: span(0),
    })
}

#[test]
fn an_origin_that_does_not_implement_the_trait_is_not_a_route() {
    let mut crates = crates();
    crates[0].extend([
        item("supplier", SUPPLIER, "::OsBackups", r::ItemKind::Type),
        item("supplier", SUPPLIER, "::Settings", r::ItemKind::Type),
        os_backups_implements(),
    ]);
    crates[1].extend([
        // An enum-like local holding both a Settings and the backups.
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::Settings")),
            local("::consume", 2),
            r::FlowKind::Construct,
        ),
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::OsBackups")),
            local("::consume", 2),
            r::FlowKind::Coerce,
        ),
        receiver(local("::consume", 2), 10),
    ]);
    let graph = graph(&workspace(), &crates, snapshot());
    let origins: Vec<_> = traced_routes(&graph)
        .into_iter()
        .map(|r| r.origin.clone())
        .collect();
    assert_eq!(origins, [item_id("supplier/lib:supplier::OsBackups")]);
}

#[test]
fn a_cross_module_call_reached_only_by_non_implementers_stops() {
    let mut crates = crates();
    crates[0].extend([
        item("supplier", SUPPLIER, "::Settings", r::ItemKind::Type),
        os_backups_implements(),
    ]);
    crates[1].extend([
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::Settings")),
            local("::consume", 2),
            r::FlowKind::Construct,
        ),
        receiver(local("::consume", 2), 10),
    ]);
    let graph = graph(&workspace(), &crates, snapshot());
    let traced = traced_routes(&graph);
    assert_eq!(traced.len(), 1);
    let RouteEnd::Stopped { reason, .. } = &traced[0].end else {
        panic!("expected a stopped route: {traced:#?}");
    };
    assert!(reason.contains("do not implement"));
}

#[test]
fn an_implementation_for_a_standard_library_type_keeps_the_filter() {
    let mut crates = crates();
    crates[0].extend([
        item("supplier", SUPPLIER, "::OsBackups", r::ItemKind::Type),
        item("supplier", SUPPLIER, "::Settings", r::ItemKind::Type),
        os_backups_implements(),
        // `impl Backups for String`
        r::Record::Impl(r::ImplRecord {
            self_type: Some(def("alloc", "a1", "::string::String")),
            contract: def("supplier", SUPPLIER, "::Backups"),
            span: span(0),
        }),
    ]);
    crates[1].extend([
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::Settings")),
            local("::consume", 2),
            r::FlowKind::Construct,
        ),
        flow(
            r::Node::Type(def("supplier", SUPPLIER, "::OsBackups")),
            local("::consume", 2),
            r::FlowKind::Coerce,
        ),
        receiver(local("::consume", 2), 10),
    ]);
    let graph = graph(&workspace(), &crates, snapshot());
    let origins: Vec<_> = traced_routes(&graph)
        .into_iter()
        .map(|r| r.origin.clone())
        .collect();
    assert_eq!(origins, [item_id("supplier/lib:supplier::OsBackups")]);
}

#[test]
fn an_outside_name_reached_through_a_declared_dependency_starts_with_that_dependency() {
    let def = r::Def {
        crate_name: "serde_core".into(),
        crate_id: "sc".into(),
        path: "::de::Deserialize".into(),
        name: "serde_core::de::Deserialize".into(),
    };
    assert_eq!(
        external_name(Some("serde"), "serde_core", &def),
        "serde::serde_core::de::Deserialize"
    );
}

#[test]
fn a_stopped_route_names_the_parameter_nothing_in_the_build_supplies() {
    let mut crates = crates();
    crates[0].push(os_backups_implements());
    crates[1].extend([
        // `consume(backups)` is only called from tests outside this build.
        flow(
            r::Node::Param {
                function: def("consumer", CONSUMER, "::consume"),
                index: 1,
            },
            local("::consume", 9),
            r::FlowKind::Copy,
        ),
        receiver(local("::consume", 9), 80),
    ]);
    let graph = graph(&workspace(), &crates, snapshot());
    let traced = traced_routes(&graph);
    assert_eq!(traced.len(), 1);
    let RouteEnd::Stopped { reason, .. } = &traced[0].end else {
        panic!("expected a stopped route: {traced:#?}");
    };
    assert_eq!(
        reason,
        "the value enters as argument 1 of `consumer::consume`, which nothing in this build supplies"
    );
}
