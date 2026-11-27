//! A small program: `app` constructs `store`'s saver and passes it into
//! `port`, which it then calls. It also logs through an external crate.

use super::*;
use graph_evaluator::{Clusters, Decision, ModuleJudgment, Relationship, RelationshipKind};
use project_graph::{
    Build, BuildUnit, CoverageGap, Dispatch, Endpoint, Fact, FactId, FactKind, Hop, ImportUse,
    Item, ItemId, ItemKind, Module, Route, RouteEnd, RouteId, SourceSpan, Support, UnitId,
    Visibility,
};
use project_map::{
    CitationId, DependencyId, HopLink, HopSupport, HopTarget, MapRouteEnd, OutsidePoint, Review,
    RouteDescription, RoutePoint, UseRole,
};
use std::collections::BTreeMap;

fn module_id(key: &str) -> ModuleId {
    ModuleId(key.into())
}

fn item(key: &str) -> Endpoint {
    Endpoint::Item(ItemId(key.into()))
}

fn fact_ref(key: &str) -> GraphRef {
    GraphRef::from(FactId(key.into()))
}

fn route_ref(key: &str) -> GraphRef {
    GraphRef::from(RouteId(key.into()))
}

fn cite(key: &str) -> CitationId {
    CitationId(key.into())
}

fn map_module(key: &str) -> project_map::ModuleId {
    project_map::ModuleId(key.into())
}

fn map_route(key: &str) -> project_map::RouteId {
    project_map::RouteId(key.into())
}

fn dependency(from: &str, to: &str) -> DependencyId {
    DependencyId {
        from: map_module(from),
        to: map_module(to),
    }
}

fn graph() -> ProjectGraph {
    let unit = UnitId("app/bin".into());
    let modules = ["app", "store", "port"]
        .map(|key| {
            let module = Module {
                name: key.into(),
                root: format!("crates/{key}"),
            };
            (module_id(key), module)
        })
        .into();
    let items = [
        ("app::main", "app", ItemKind::Function, Visibility::Exported),
        (
            "store::FileSaver",
            "store",
            ItemKind::Type,
            Visibility::Exported,
        ),
        (
            "store::helper",
            "store",
            ItemKind::Function,
            Visibility::Internal,
        ),
        ("port::Saver", "port", ItemKind::Type, Visibility::Exported),
    ]
    .map(|(key, module, kind, visibility)| {
        let item = Item {
            module: module_id(module),
            name: key.rsplit("::").next().unwrap().into(),
            kind,
            visibility,
            signature: None,
            declaration: Some(SourceSpan {
                file: format!("crates/{module}/src/lib.rs"),
                bytes: 0..10,
            }),
        };
        (ItemId(key.into()), item)
    })
    .into();
    let fact = |subject: Endpoint, kind: FactKind, object: Endpoint, at: usize| Fact {
        unit: unit.clone(),
        subject,
        kind,
        object,
        site: Some(SourceSpan {
            file: "crates/app/src/main.rs".into(),
            bytes: at..at + 5,
        }),
    };
    let facts = [
        (
            "import",
            fact(
                item("app::main"),
                FactKind::Import {
                    local_name: "FileSaver".into(),
                    usage: ImportUse::Value,
                },
                item("store::FileSaver"),
                0,
            ),
        ),
        (
            "implements",
            fact(
                item("store::FileSaver"),
                FactKind::Implements,
                item("port::Saver"),
                10,
            ),
        ),
        (
            "call",
            fact(
                item("app::main"),
                FactKind::Call(Dispatch::Direct),
                Endpoint::Module(module_id("port")),
                20,
            ),
        ),
        (
            "construct",
            fact(
                item("store::FileSaver"),
                FactKind::Construct,
                item("app::main"),
                30,
            ),
        ),
        (
            "pass",
            fact(
                Endpoint::Place {
                    owner: ItemId("app::main".into()),
                    projection: "saver".into(),
                },
                FactKind::Pass,
                Endpoint::Place {
                    owner: ItemId("port::Saver".into()),
                    projection: "self".into(),
                },
                40,
            ),
        ),
        (
            "log",
            fact(
                item("app::main"),
                FactKind::Call(Dispatch::Foreign),
                Endpoint::External("log::info".into()),
                50,
            ),
        ),
    ]
    .map(|(key, fact)| (FactId(key.into()), fact))
    .into();
    let routes = [
        (
            "inject",
            Route {
                use_sites: vec![FactId("call".into())],
                origin: item("store::FileSaver"),
                hops: vec![
                    Hop {
                        to: item("app::main"),
                        facts: vec![FactId("construct".into())],
                        support: Support::Established,
                    },
                    Hop {
                        to: Endpoint::Place {
                            owner: ItemId("port::Saver".into()),
                            projection: "self".into(),
                        },
                        facts: vec![FactId("pass".into())],
                        support: Support::Possible {
                            condition: Some("feature files".into()),
                        },
                    },
                ],
                end: RouteEnd::ReachedUse,
            },
        ),
        (
            "log",
            Route {
                use_sites: vec![FactId("log".into())],
                origin: item("app::main"),
                hops: vec![Hop {
                    to: Endpoint::External("log::info".into()),
                    facts: vec![FactId("log".into())],
                    support: Support::Established,
                }],
                end: RouteEnd::ReachedUse,
            },
        ),
        (
            "stopped",
            Route {
                use_sites: vec![FactId("call".into())],
                origin: item("app::main"),
                hops: vec![],
                end: RouteEnd::Stopped {
                    at: vec![FactId("call".into())],
                    reason: "the receiver was not traced".into(),
                },
            },
        ),
    ]
    .map(|(key, route)| (RouteId(key.into()), route))
    .into();
    ProjectGraph {
        id: GraphId("app@abc123".into()),
        build: Build {
            project: "app".into(),
            source_revision: "abc123".into(),
            target: "app/bin".into(),
            configuration: "default features".into(),
        },
        modules,
        units: BTreeMap::from([(
            unit.clone(),
            BuildUnit {
                module: module_id("app"),
                target: "bin".into(),
                settings: BTreeMap::new(),
            },
        )]),
        items,
        facts,
        routes,
        gaps: vec![CoverageGap {
            unit: Some(unit),
            site: None,
            reason: "macro bodies were not expanded".into(),
        }],
    }
}

fn relationship(kind: RelationshipKind, reason: &str, evidence: Vec<GraphRef>) -> Relationship {
    Relationship {
        kind,
        reason: reason.into(),
        evidence,
    }
}

fn evaluation() -> GraphEvaluation {
    let program = ClusterName("Program".into());
    let judged = |concern: &str, evidence: Vec<GraphRef>| Decision::Judged {
        judgment: ModuleJudgment {
            concern: concern.into(),
            kind: graph_evaluator::ModuleKind::Composition,
            cluster: program.clone(),
        },
        reason: format!("{concern} because of the cited code"),
        evidence,
    };
    GraphEvaluation {
        graph: GraphId("app@abc123".into()),
        clusters: Clusters::new(vec![program.clone(), ClusterName("Core".into())]).unwrap(),
        modules: BTreeMap::from([
            (
                module_id("app"),
                judged(
                    "Runs the program",
                    vec![GraphRef::from(ItemId("app::main".into()))],
                ),
            ),
            (
                module_id("store"),
                Decision::Deferred {
                    reason: "needs the storage design".into(),
                },
            ),
            (module_id("ghost"), judged("Haunts", vec![])),
        ]),
        relationships: vec![
            relationship(
                RelationshipKind::FullInjection,
                "app passes the saver into port whole",
                vec![route_ref("inject"), fact_ref("pass")],
            ),
            relationship(
                RelationshipKind::DirectUse,
                "app imports the saver",
                vec![fact_ref("import")],
            ),
            relationship(
                RelationshipKind::Incidental,
                "logging is incidental",
                vec![fact_ref("log")],
            ),
            relationship(
                RelationshipKind::Wires,
                "cites nothing real",
                vec![fact_ref("nope")],
            ),
        ],
    }
}

fn built() -> MapBuild {
    build_map(&graph(), &evaluation()).unwrap()
}

#[test]
fn an_evaluation_of_another_graph_is_refused() {
    let mut evaluation = evaluation();
    evaluation.graph = GraphId("app@other".into());
    let error = build_map(&graph(), &evaluation).unwrap_err();
    assert_eq!(error.evaluated, GraphId("app@other".into()));
    assert_eq!(error.graph, GraphId("app@abc123".into()));
}

#[test]
fn the_built_map_is_consistent_and_names_its_build() {
    let map = built().map;
    assert_eq!(map.problems(), vec![]);
    let source = map.source.unwrap();
    assert_eq!(source.graph_id.0, "app@abc123");
    assert_eq!(source.source_revision.0, "abc123");
    assert_eq!(source.target, "app/bin");
    assert_eq!(map.project, "app");
    let layers: Vec<_> = map.layers.iter().map(|layer| layer.0.as_str()).collect();
    assert_eq!(layers, ["Program", "Core"]);
}

#[test]
fn problems_report_judgments_whose_subjects_are_missing() {
    assert_eq!(
        built().problems,
        vec![
            BuildProblem::UnknownModule(module_id("ghost")),
            BuildProblem::UnresolvedRelationshipCitation {
                relationship: 3,
                reference: fact_ref("nope"),
            },
            BuildProblem::UnplacedRelationship { relationship: 3 },
        ]
    );
}

#[test]
fn modules_keep_judged_deferred_and_unreviewed_reviews() {
    let map = built().map;
    let Review::Judged {
        judgment,
        primary,
        supporting,
        ..
    } = &map.modules[&map_module("app")].description
    else {
        panic!("app is judged");
    };
    assert_eq!(judgment.concern, "Runs the program");
    assert_eq!(judgment.layer.as_ref().unwrap().0, "Program");
    assert_eq!(primary, &cite("item:app::main"));
    assert!(supporting.is_empty());
    assert!(map.evidence[primary].location.is_some());
    assert_eq!(
        map.modules[&map_module("store")].description,
        Review::Deferred {
            reason: "needs the storage design".into()
        }
    );
    assert_eq!(
        map.modules[&map_module("port")].description,
        Review::Unreviewed
    );
    assert!(!map.modules.contains_key(&map_module("ghost")));
}

#[test]
fn the_surface_lists_only_exported_items() {
    let map = built().map;
    let names: Vec<_> = map.modules[&map_module("store")]
        .surface
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    assert_eq!(names, ["FileSaver"]);
}

#[test]
fn the_surface_leaves_out_methods_and_submodules_which_the_graph_keeps() {
    let mut graph = graph();
    for (key, name, kind) in [
        (
            "store::FileSaver::save",
            "FileSaver::save",
            ItemKind::Method,
        ),
        (
            "store::<FileSaver as Clone>::clone",
            "<FileSaver as Clone>::clone",
            ItemKind::Method,
        ),
        ("store::files", "files", ItemKind::Module),
        ("store::LIMIT", "LIMIT", ItemKind::Value),
        ("store::open", "open", ItemKind::Function),
    ] {
        graph.items.insert(
            ItemId(key.into()),
            Item {
                module: module_id("store"),
                name: name.into(),
                kind,
                visibility: Visibility::Exported,
                signature: None,
                declaration: None,
            },
        );
    }
    let built = build_map(&graph, &evaluation()).unwrap();
    let surface: Vec<_> = built.map.modules[&map_module("store")]
        .surface
        .iter()
        .map(|item| (item.name.as_str(), item.kind.clone()))
        .collect();
    assert_eq!(
        surface,
        [
            ("FileSaver", project_map::ItemKind::Type),
            ("LIMIT", project_map::ItemKind::Value),
            ("open", project_map::ItemKind::Function),
        ]
    );
}

#[test]
fn a_judgment_citing_nothing_in_the_graph_rests_on_its_module() {
    let mut evaluation = evaluation();
    evaluation.modules.insert(
        module_id("port"),
        Decision::Judged {
            judgment: ModuleJudgment {
                concern: "Defines saving".into(),
                kind: graph_evaluator::ModuleKind::Contract,
                cluster: ClusterName("Elsewhere".into()),
            },
            reason: "named port".into(),
            evidence: vec![fact_ref("missing")],
        },
    );
    let build = build_map(&graph(), &evaluation).unwrap();
    assert!(
        build
            .problems
            .contains(&BuildProblem::UnresolvedModuleCitation {
                module: module_id("port"),
                reference: fact_ref("missing"),
            })
    );
    assert!(build.problems.contains(&BuildProblem::UnlistedCluster {
        module: module_id("port"),
        cluster: ClusterName("Elsewhere".into()),
    }));
    let Review::Judged {
        judgment, primary, ..
    } = &build.map.modules[&map_module("port")].description
    else {
        panic!("port is judged");
    };
    assert_eq!(judgment.layer, None);
    assert_eq!(primary, &cite("module:port"));
    assert_eq!(
        build.map.evidence[primary].summary,
        "Module port at crates/port."
    );
    assert_eq!(build.map.problems(), vec![]);
}

#[test]
fn use_facts_and_hops_make_dependencies() {
    let map = built().map;
    let keys: Vec<_> = map.dependencies.keys().cloned().collect();
    assert_eq!(
        keys,
        [
            dependency("app", "port"),
            dependency("app", "store"),
            dependency("store", "port"),
        ]
    );
    // The hops add their value moves to the dependencies they travel along.
    assert_eq!(
        map.dependencies[&dependency("app", "store")].evidence,
        [cite("fact:import"), cite("fact:construct")]
    );
    assert_eq!(
        map.dependencies[&dependency("app", "port")].evidence,
        [cite("fact:call"), cite("fact:pass")]
    );
}

#[test]
fn hops_say_how_they_reach_each_module_and_keep_their_support() {
    let map = built().map;
    let route = &map.routes[&map_route("inject")];
    assert_eq!(
        route.origin,
        RoutePoint::Project(project_map::ProjectPoint {
            module: map_module("store"),
            item: Some("FileSaver".into()),
        })
    );
    let links: Vec<_> = route
        .hops
        .iter()
        .map(|hop| match &hop.to {
            HopTarget::Project { link, .. } => link.clone(),
            HopTarget::Outside(_) => panic!("both hops stay in the project"),
        })
        .collect();
    assert_eq!(
        links,
        [HopLink::ReceiverUsesSender, HopLink::SenderUsesReceiver]
    );
    assert_eq!(route.hops[0].support, HopSupport::Established);
    assert_eq!(
        route.hops[1].support,
        HopSupport::Possible {
            condition: Some("feature files".into())
        }
    );
    let HopTarget::Project { point, .. } = &route.hops[1].to else {
        unreachable!()
    };
    assert_eq!(point.item.as_deref(), Some("Saver.self"));
    assert_eq!(route.use_sites, [cite("fact:call")]);
    assert_eq!(
        map.routes[&map_route("log")].hops[0].to,
        HopTarget::Outside(OutsidePoint::External("log::info".into()))
    );
}

#[test]
fn a_stopped_trace_keeps_where_and_why_it_stopped() {
    let map = built().map;
    let route = &map.routes[&map_route("stopped")];
    assert!(route.hops.is_empty());
    assert_eq!(
        route.end,
        MapRouteEnd::Stopped {
            at: vec![cite("fact:call")],
            reason: "the receiver was not traced".into(),
        }
    );
    assert_eq!(route.description, Review::Unreviewed);
}

#[test]
fn relationships_review_the_routes_hops_and_dependencies_they_cite() {
    let map = built().map;
    let injection = Review::Judged {
        judgment: RouteDescription {
            role: UseRole::FullInjection,
            other_roles: vec![],
        },
        reason: "app passes the saver into port whole".into(),
        primary: cite("route:inject"),
        supporting: vec![cite("fact:pass")],
    };
    let route = &map.routes[&map_route("inject")];
    assert_eq!(route.description, injection);
    assert_eq!(route.hops[0].role, None);
    assert_eq!(route.hops[1].role, Some(UseRole::FullInjection));
    // The route travels two dependencies, so it reviews only the one whose
    // fact the relationship cites.
    assert_eq!(
        map.dependencies[&dependency("app", "port")].description,
        injection
    );
    let Review::Judged { judgment, .. } =
        &map.dependencies[&dependency("app", "store")].description
    else {
        panic!("the import is judged");
    };
    assert_eq!(judgment.role, UseRole::DirectUse);
    assert_eq!(
        map.dependencies[&dependency("store", "port")].description,
        Review::Unreviewed
    );
    assert_eq!(
        map.routes[&map_route("log")].hops[0].role,
        Some(UseRole::Incidental)
    );
    assert!(map.evidence.contains_key(&cite("route:inject")));
}

#[test]
fn several_relationships_on_one_subject_combine_in_evaluation_order() {
    let mut evaluation = evaluation();
    evaluation.relationships.push(relationship(
        RelationshipKind::Wires,
        "app also wires it",
        vec![fact_ref("construct"), route_ref("inject")],
    ));
    let map = build_map(&graph(), &evaluation).unwrap().map;
    assert_eq!(
        map.routes[&map_route("inject")].description,
        Review::Judged {
            judgment: RouteDescription {
                role: UseRole::FullInjection,
                other_roles: vec![UseRole::Wires],
            },
            reason: "app passes the saver into port whole\napp also wires it".into(),
            primary: cite("route:inject"),
            supporting: vec![cite("fact:pass"), cite("fact:construct")],
        }
    );
    assert_eq!(map.problems(), vec![]);
}

#[test]
fn a_route_along_one_dependency_reviews_that_dependency() {
    let mut evaluation = evaluation();
    evaluation.relationships = vec![relationship(
        RelationshipKind::Implements,
        "the saver implements the port",
        vec![route_ref("implemented")],
    )];
    let mut graph = graph();
    graph.routes.insert(
        RouteId("implemented".into()),
        Route {
            use_sites: Vec::new(),
            origin: item("port::Saver"),
            hops: vec![Hop {
                to: item("store::FileSaver"),
                facts: vec![FactId("implements".into())],
                support: Support::Established,
            }],
            end: RouteEnd::ReachedUse,
        },
    );
    let build = build_map(&graph, &evaluation).unwrap();
    assert_eq!(
        build.problems,
        vec![BuildProblem::UnknownModule(module_id("ghost"))]
    );
    let Review::Judged { judgment, .. } =
        &build.map.dependencies[&dependency("store", "port")].description
    else {
        panic!("the dependency is judged through its route");
    };
    assert_eq!(judgment.role, UseRole::Implements);
}

#[test]
fn a_value_move_with_no_use_behind_it_picks_its_direction_from_the_move() {
    let mut graph = graph();
    graph.facts.insert(
        FactId("returned".into()),
        Fact {
            unit: UnitId("app/bin".into()),
            subject: item("port::Saver"),
            kind: FactKind::Return,
            object: item("store::helper"),
            site: None,
        },
    );
    graph.routes.insert(
        RouteId("returned".into()),
        Route {
            use_sites: Vec::new(),
            origin: item("port::Saver"),
            hops: vec![Hop {
                to: item("store::helper"),
                facts: vec![FactId("returned".into())],
                support: Support::Established,
            }],
            end: RouteEnd::ReachedUse,
        },
    );
    let map = build_map(&graph, &evaluation()).unwrap().map;
    // store already uses port through `implements`, so the hop travels it.
    let HopTarget::Project { link, .. } = &map.routes[&map_route("returned")].hops[0].to else {
        panic!("the hop stays in the project")
    };
    assert_eq!(link, &HopLink::ReceiverUsesSender);
    assert_eq!(
        map.dependencies[&dependency("store", "port")].evidence,
        [cite("fact:implements"), cite("fact:returned")]
    );

    // Without that use, the return alone makes the receiver use the sender.
    graph.facts.remove(&FactId("implements".into()));
    let map = build_map(&graph, &evaluation()).unwrap().map;
    assert_eq!(
        map.dependencies[&dependency("store", "port")].evidence,
        [cite("fact:returned")]
    );
    assert_eq!(map.problems(), vec![]);
}

#[test]
fn references_the_graph_cannot_resolve_are_reported_and_kept_visible() {
    let mut graph = graph();
    graph.routes.insert(
        RouteId("broken".into()),
        Route {
            use_sites: Vec::new(),
            origin: item("gone::Thing"),
            hops: vec![Hop {
                to: item("app::main"),
                facts: vec![FactId("vanished".into())],
                support: Support::Established,
            }],
            end: RouteEnd::ReachedUse,
        },
    );
    let build = build_map(&graph, &evaluation()).unwrap();
    let broken = route_ref("broken");
    assert!(build.problems.contains(&BuildProblem::IncompleteGraph {
        cited_by: broken.clone(),
        missing: GraphRef::from(ItemId("gone::Thing".into())),
    }));
    assert!(build.problems.contains(&BuildProblem::IncompleteGraph {
        cited_by: broken,
        missing: fact_ref("vanished"),
    }));
    let route = &build.map.routes[&map_route("broken")];
    assert_eq!(
        route.origin,
        RoutePoint::Outside(OutsidePoint::Unresolved(
            "gone::Thing (missing from the graph)".into()
        ))
    );
    let HopTarget::Project { link, .. } = &route.hops[0].to else {
        panic!("app is in the project")
    };
    assert_eq!(link, &HopLink::FromOutside);
    assert_eq!(
        build.map.evidence[&cite("fact:vanished")].summary,
        "Fact vanished is missing from the graph."
    );
    assert_eq!(build.map.problems(), vec![]);
}

#[test]
fn facts_and_gaps_become_evidence_with_their_sources() {
    let map = built().map;
    let import = &map.evidence[&cite("fact:import")];
    assert_eq!(
        import.summary,
        "main imports FileSaver as FileSaver (used as a value)."
    );
    assert_eq!(import.build_unit.as_ref().unwrap().0, "app/bin");
    let location = import.location.as_ref().unwrap();
    assert_eq!(location.file.0, "crates/app/src/main.rs");
    assert_eq!(location.bytes, 0..5);
    assert_eq!(map.coverage_gaps.len(), 1);
    assert_eq!(
        map.coverage_gaps[0].summary,
        "macro bodies were not expanded"
    );
}

#[test]
fn a_relationship_that_only_reaches_a_hop_taken_by_an_earlier_one_is_reported() {
    let mut evaluation = evaluation();
    evaluation.relationships.push(relationship(
        RelationshipKind::DirectUse,
        "logging is a direct use too",
        vec![fact_ref("log")],
    ));
    let build = build_map(&graph(), &evaluation).unwrap();
    assert!(
        build
            .problems
            .contains(&BuildProblem::UnplacedRelationship { relationship: 4 })
    );
    assert!(
        !build
            .problems
            .contains(&BuildProblem::UnplacedRelationship { relationship: 2 })
    );
    assert_eq!(
        build.map.routes[&map_route("log")].hops[0].role,
        Some(UseRole::Incidental)
    );
}

#[test]
fn a_cluster_listed_twice_is_reported_and_keeps_its_first_place() {
    let clusters = ["Program", "Core", "Program"].map(|name| ClusterName(name.into()));
    let mut problems = vec![];
    let layers = layers(clusters.iter(), &mut problems);
    let names: Vec<_> = layers.iter().map(|layer| layer.0.as_str()).collect();
    assert_eq!(names, ["Program", "Core"]);
    assert_eq!(
        problems,
        [BuildProblem::DuplicateCluster(ClusterName(
            "Program".into()
        ))]
    );
}

#[test]
fn a_route_merged_from_several_sites_keeps_every_site_and_fact() {
    let mut graph = graph();
    let again = |fact: &str, at: usize| {
        let mut copy = graph.facts[&FactId(fact.into())].clone();
        copy.site.as_mut().unwrap().bytes = at..at + 5;
        copy
    };
    let (construct, log) = (again("construct", 60), again("log", 70));
    graph
        .facts
        .insert(FactId("construct-again".into()), construct);
    graph.facts.insert(FactId("log-again".into()), log);
    let route = graph.routes.get_mut(&RouteId("inject".into())).unwrap();
    route.hops[0].facts.push(FactId("construct-again".into()));
    let route = graph.routes.get_mut(&RouteId("log".into())).unwrap();
    route.use_sites.push(FactId("log-again".into()));
    route.hops[0].facts.push(FactId("log-again".into()));
    let mut evaluation = evaluation();
    // Cites only the second site's fact, which still reaches the hop.
    evaluation.relationships[2].evidence = vec![fact_ref("log-again")];

    let build = build_map(&graph, &evaluation).unwrap();
    let log = &build.map.routes[&map_route("log")];
    assert_eq!(log.use_sites, [cite("fact:log"), cite("fact:log-again")]);
    assert_eq!(
        log.hops[0].evidence,
        [cite("fact:log"), cite("fact:log-again")]
    );
    assert_eq!(log.hops[0].role, Some(UseRole::Incidental));
    assert!(
        !build
            .problems
            .contains(&BuildProblem::UnplacedRelationship { relationship: 2 })
    );
    let inject = &build.map.routes[&map_route("inject")];
    assert_eq!(
        inject.hops[0].evidence,
        [cite("fact:construct"), cite("fact:construct-again")]
    );
    let evidence = &build.map.dependencies[&dependency("app", "store")].evidence;
    assert!(evidence.contains(&cite("fact:construct")));
    assert!(evidence.contains(&cite("fact:construct-again")));
    assert_eq!(build.map.problems(), vec![]);
}

#[test]
fn a_stopped_end_cites_every_place_its_uses_stopped() {
    let mut graph = graph();
    let route = graph.routes.get_mut(&RouteId("stopped".into())).unwrap();
    route.use_sites.push(FactId("log".into()));
    route.end = RouteEnd::Stopped {
        at: vec![FactId("call".into()), FactId("log".into())],
        reason: "the receiver was not traced".into(),
    };
    let map = build_map(&graph, &evaluation()).unwrap().map;
    assert_eq!(
        map.routes[&map_route("stopped")].end,
        MapRouteEnd::Stopped {
            at: vec![cite("fact:call"), cite("fact:log")],
            reason: "the receiver was not traced".into(),
        }
    );
}
