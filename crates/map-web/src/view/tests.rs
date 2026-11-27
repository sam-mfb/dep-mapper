use super::*;
use project_map::example::sharsheret;
use project_map::*;

fn module(review: Review<ModuleDescription>) -> MapModule {
    MapModule {
        status: Status::Observed,
        name: "ignored".into(),
        surface: vec![],
        description: review,
    }
}

fn judged_module(kind: ModuleKind, layer: &str) -> MapModule {
    module(Review::Judged {
        judgment: ModuleDescription {
            concern: "A concern.".into(),
            kind,
            layer: Some(LayerName(layer.into())),
        },
        reason: "Because.".into(),
        primary: CitationId("fact".into()),
        supporting: vec![],
    })
}

/// A map with an unreviewed module, an unlisted layer, an unreviewed
/// dependency, a role the reviewer named, and a deferred route that leaves
/// the project and stops.
fn mixed_map() -> ProjectMap {
    let fact = CitationId("fact".into());
    ProjectMap {
        project: "p".into(),
        source: Some(MapSource {
            graph_id: GraphId("g".into()),
            source_revision: SourceRevision("r".into()),
            target: "t".into(),
            configuration: "c".into(),
        }),
        layers: Layers::new(vec![LayerName("Core".into())]).unwrap(),
        modules: BTreeMap::from([
            (
                ModuleId("core".into()),
                judged_module(ModuleKind::Logic, "Core"),
            ),
            (
                ModuleId("stray".into()),
                judged_module(ModuleKind::Effect, "Elsewhere"),
            ),
            (ModuleId("new".into()), module(Review::Unreviewed)),
        ]),
        dependencies: BTreeMap::from([
            (
                DependencyId {
                    from: ModuleId("new".into()),
                    to: ModuleId("core".into()),
                },
                MapDependency {
                    status: Status::Observed,
                    evidence: vec![fact.clone()],
                    description: Review::Unreviewed,
                },
            ),
            (
                DependencyId {
                    from: ModuleId("stray".into()),
                    to: ModuleId("core".into()),
                },
                MapDependency {
                    status: Status::Observed,
                    evidence: vec![fact.clone()],
                    description: Review::Judged {
                        judgment: RouteDescription {
                            role: UseRole::DirectUse,
                            other_roles: vec![UseRole::Other("logs through".into())],
                        },
                        reason: "Calls and logs.".into(),
                        primary: fact.clone(),
                        supporting: vec![],
                    },
                },
            ),
        ]),
        routes: BTreeMap::from([(
            RouteId("open".into()),
            MapRoute {
                status: Status::Observed,
                use_sites: Vec::new(),
                origin: RoutePoint::Project(ProjectPoint {
                    module: ModuleId("core".into()),
                    item: Some("Store".into()),
                }),
                hops: vec![
                    MapHop {
                        to: HopTarget::Outside(OutsidePoint::External("tokio".into())),
                        evidence: vec![fact.clone()],
                        support: HopSupport::Possible {
                            condition: Some("async build".into()),
                        },
                        role: None,
                    },
                    MapHop {
                        to: HopTarget::Project {
                            point: ProjectPoint {
                                module: ModuleId("new".into()),
                                item: None,
                            },
                            link: HopLink::FromOutside,
                        },
                        evidence: vec![fact.clone()],
                        support: HopSupport::Established,
                        role: None,
                    },
                ],
                end: MapRouteEnd::Stopped {
                    at: vec![fact.clone()],
                    reason: "Unknown receiver.".into(),
                },
                description: Review::Deferred {
                    reason: "Needs the receiver.".into(),
                },
            },
        )]),
        evidence: BTreeMap::from([(
            fact,
            MapEvidence {
                kind: EvidenceKind::Source,
                build_unit: Some(UnitId("core/lib".into())),
                summary: "core defines Store.".into(),
                location: Some(Location {
                    file: SourcePath("src/lib.rs".into()),
                    bytes: 10..20,
                }),
            },
        )]),
        coverage_gaps: vec![],
    }
}

fn crate_named<'a>(view: &'a View, name: &str) -> &'a Crate {
    view.crates.iter().find(|c| c.name == name).expect(name)
}

#[test]
fn the_example_keeps_every_module_dependency_and_route() {
    let view = view(&sharsheret());
    assert_eq!(
        (view.crates.len(), view.edges.len(), view.routes.len()),
        (12, 23, 11)
    );
    assert_eq!(view.layers, ["Program", "Map", "Evaluation", "Evidence"]);
    assert!(view.crates.iter().all(|c| c.review.status == "judged"));
    let programs: Vec<&str> = view
        .crates
        .iter()
        .filter(|c| c.program)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(programs, ["mapper"]);
}

/// A route that carries `Thing` from `a` through the listed modules.
fn route_through(modules: &[&str], description: Review<RouteDescription>) -> MapRoute {
    let fact = CitationId("fact".into());
    let hops = modules
        .iter()
        .enumerate()
        .map(|(index, module)| MapHop {
            to: HopTarget::Project {
                point: ProjectPoint {
                    module: ModuleId((*module).into()),
                    item: None,
                },
                link: if index == 0 {
                    HopLink::ReceiverUsesSender
                } else {
                    HopLink::SenderUsesReceiver
                },
            },
            evidence: vec![fact.clone()],
            support: HopSupport::Established,
            role: Some(if index == 0 {
                UseRole::Wires
            } else {
                UseRole::SuppliesInput
            }),
        })
        .collect();
    MapRoute {
        status: Status::Observed,
        use_sites: vec![fact.clone()],
        origin: RoutePoint::Project(ProjectPoint {
            module: ModuleId("a".into()),
            item: Some("Thing".into()),
        }),
        hops,
        end: MapRouteEnd::ReachedUse,
        description,
    }
}

fn judged_route(role: UseRole) -> Review<RouteDescription> {
    Review::Judged {
        judgment: RouteDescription {
            role,
            other_roles: vec![],
        },
        reason: "Because.".into(),
        primary: CitationId("fact".into()),
        supporting: vec![],
    }
}

#[test]
fn only_judged_injections_through_three_modules_are_drawn() {
    let mut map = mixed_map();
    map.routes = BTreeMap::from([
        (
            RouteId("full".into()),
            route_through(&["b", "c"], judged_route(UseRole::FullInjection)),
        ),
        (
            RouteId("adapted".into()),
            route_through(&["b", "c"], judged_route(UseRole::AdaptedInjection)),
        ),
        (
            RouteId("too-short".into()),
            route_through(&["b"], judged_route(UseRole::FullInjection)),
        ),
        (
            RouteId("direct".into()),
            route_through(&["b", "c"], judged_route(UseRole::DirectUse)),
        ),
        (
            RouteId("unreviewed".into()),
            route_through(&["b", "c"], Review::Unreviewed),
        ),
    ]);
    let view = view(&map);
    let drawn: Vec<&str> = view
        .routes
        .iter()
        .filter(|r| r.injection)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(drawn, ["adapted", "full"]);
    let route = view.routes.iter().find(|r| r.id == "full").unwrap();
    assert_eq!(route.modules, ["a", "b", "c"]);
    assert_eq!(
        (route.carry.as_str(), route.first_role.as_str()),
        ("Thing", "wires")
    );
}

#[test]
fn a_contract_groups_its_implementations_in_the_same_layer() {
    let view = view(&sharsheret());
    assert_eq!(view.groups.current, view.groups.planned);
    assert_eq!(
        view.groups.current,
        BTreeMap::from([
            (
                "graph-evaluator".to_string(),
                vec![
                    "graph-evaluator-llm".to_string(),
                    "graph-evaluator-manual".to_string()
                ]
            ),
            (
                "project-graph".to_string(),
                vec![
                    "project-graph-rust".to_string(),
                    "project-graph-typescript".to_string()
                ]
            ),
        ])
    );
}

#[test]
fn modules_without_a_listed_layer_go_to_the_unassigned_band() {
    let view = view(&mixed_map());
    assert_eq!(view.layers, ["Core", UNASSIGNED_LAYER]);
    let new = crate_named(&view, "new");
    assert_eq!(
        (new.kind.as_str(), new.layer.as_str()),
        ("unreviewed", UNASSIGNED_LAYER)
    );
    assert_eq!(new.review.status, "unreviewed");
    assert_eq!(crate_named(&view, "stray").layer, UNASSIGNED_LAYER);
    assert_eq!(crate_named(&view, "core").layer, "Core");
}

#[test]
fn a_dependency_keeps_its_review_status_and_named_roles() {
    let view = view(&mixed_map());
    let unreviewed = view.edges.iter().find(|e| e.from == "new").unwrap();
    assert_eq!(unreviewed.roles, ["unreviewed"]);
    assert_eq!(unreviewed.review.status, "unreviewed");
    let named = view.edges.iter().find(|e| e.from == "stray").unwrap();
    assert_eq!(named.roles, ["uses", "other"]);
    assert_eq!(named.other_roles, ["logs through"]);
    assert_eq!(named.review.primary.as_deref(), Some("fact"));
}

#[test]
fn a_deferred_route_keeps_its_possible_hop_external_point_and_stop() {
    let view = view(&mixed_map());
    let route = &view.routes[0];
    assert!(!route.injection);
    assert_eq!(route.modules, ["core", "new"]);
    assert_eq!(route.carry, "Store");
    assert_eq!(route.first_role, "unreviewed");
    assert_eq!(route.hops[0].to.module, None);
    assert_eq!(route.hops[0].to.label, "tokio (external)");
    assert_eq!(route.hops[0].support, "possible");
    assert_eq!(route.hops[0].condition.as_deref(), Some("async build"));
    assert_eq!(route.hops[1].support, "established");
    assert!(!route.end.reached);
    assert_eq!(route.end.at, ["fact"]);
    assert_eq!(route.end.reason.as_deref(), Some("Unknown receiver."));
    assert_eq!(route.review.status, "deferred");
    assert_eq!(route.review.reason.as_deref(), Some("Needs the receiver."));
}

#[test]
fn evidence_keeps_its_build_unit_and_location() {
    let view = view(&mixed_map());
    let fact = &view.evidence["fact"];
    assert_eq!(fact.unit.as_deref(), Some("core/lib"));
    let location = fact.location.as_ref().unwrap();
    assert_eq!(
        (location.file.as_str(), location.start, location.end),
        ("src/lib.rs", 10, 20)
    );
}

#[test]
fn a_proposed_hop_is_marked_proposed() {
    let view = view(&sharsheret());
    let route = view
        .routes
        .iter()
        .find(|r| r.id == project_map::example::PROPOSED_ROUTE)
        .unwrap();
    assert!(route.hops.iter().all(|hop| hop.support == "proposed"));
    assert!(route.hops.iter().all(|hop| hop.condition.is_none()));
}

#[test]
fn the_source_build_is_kept_and_a_map_without_one_has_none() {
    let mut map = mixed_map();
    let source = view(&map).meta.source.expect("the fixture has a source");
    assert_eq!(
        [
            source.graph_id,
            source.revision,
            source.target,
            source.configuration
        ],
        ["g", "r", "t", "c"]
    );
    map.source = None;
    assert!(view(&map).meta.source.is_none());
}

#[test]
fn each_part_keeps_its_status_and_each_note_its_kind() {
    let mut map = mixed_map();
    map.modules.get_mut(&ModuleId("new".into())).unwrap().status = Status::ProposedAddition;
    map.modules
        .get_mut(&ModuleId("stray".into()))
        .unwrap()
        .status = Status::ProposedRemoval;
    for dependency in map.dependencies.values_mut() {
        dependency.status = Status::ProposedAddition;
    }
    map.routes.get_mut(&RouteId("open".into())).unwrap().status = Status::ProposedRemoval;
    map.evidence.insert(
        CitationId("note".into()),
        MapEvidence {
            kind: EvidenceKind::Proposal,
            build_unit: None,
            summary: "Why.".into(),
            location: None,
        },
    );
    let view = view(&map);
    let statuses: Vec<(&str, &str)> = view
        .crates
        .iter()
        .map(|c| (c.name.as_str(), c.status))
        .collect();
    assert_eq!(
        statuses,
        [
            ("core", "observed"),
            ("new", "addition"),
            ("stray", "removal")
        ]
    );
    assert!(
        view.edges
            .iter()
            .all(|e| e.status == "addition" && e.update.is_none())
    );
    assert_eq!(view.routes[0].status, "removal");
    assert_eq!(view.evidence["fact"].kind, "source");
    assert_eq!(view.evidence["note"].kind, "proposal");
}

#[test]
fn an_update_keeps_the_part_in_code_and_lists_what_changes() {
    let mut map = mixed_map();
    let core = map.modules.get_mut(&ModuleId("core".into())).unwrap();
    let mut proposed = core.description.clone();
    if let Review::Judged { judgment, .. } = &mut proposed {
        judgment.kind = ModuleKind::Effect;
        judgment.layer = Some(LayerName("Elsewhere".into()));
    }
    core.status = Status::ProposedUpdate { proposed };
    let stray = DependencyId {
        from: ModuleId("stray".into()),
        to: ModuleId("core".into()),
    };
    let dependency = map.dependencies.get_mut(&stray).unwrap();
    dependency.status = Status::ProposedUpdate {
        proposed: Review::Deferred {
            reason: "Unsure.".into(),
        },
    };
    let view = view(&map);
    let core = crate_named(&view, "core");
    assert_eq!((core.status, core.kind.as_str()), ("update", "logic"));
    let update = core.update.as_ref().expect("an update");
    assert_eq!(update.kind, "effect");
    // "Elsewhere" is not a listed layer, so the module would be unassigned.
    assert_eq!(update.layer, UNASSIGNED_LAYER);
    let changes: Vec<(&str, &str, &str)> = update
        .changes
        .iter()
        .map(|c| (c.field, c.before.as_str(), c.after.as_str()))
        .collect();
    assert_eq!(
        changes,
        [
            ("kind", "logic", "effect"),
            ("layer", "Core", UNASSIGNED_LAYER)
        ]
    );
    let edge = view.edges.iter().find(|e| e.from == "stray").unwrap();
    let update = edge.update.as_ref().expect("an update");
    assert_eq!(update.roles, ["unreviewed"]);
    let changes: Vec<(&str, &str, &str)> = update
        .changes
        .iter()
        .map(|c| (c.field, c.before.as_str(), c.after.as_str()))
        .collect();
    assert_eq!(
        changes,
        [
            ("roles", "direct use, other: logs through", ""),
            ("review", "judged", "deferred")
        ]
    );
}

/// A contract `port` and an `adapter` in one layer, the adapter's dependency
/// on the port judged with `role` and marked with `status`.
fn port_and_adapter(role: UseRole, status: Status<RouteDescription>) -> ProjectMap {
    let mut map = mixed_map();
    map.layers = Layers::new(vec![LayerName("Core".into())]).unwrap();
    map.modules = BTreeMap::from([
        (
            ModuleId("port".into()),
            judged_module(ModuleKind::Contract, "Core"),
        ),
        (
            ModuleId("adapter".into()),
            judged_module(ModuleKind::Effect, "Core"),
        ),
    ]);
    map.dependencies = BTreeMap::from([(
        DependencyId {
            from: ModuleId("adapter".into()),
            to: ModuleId("port".into()),
        },
        MapDependency {
            status,
            evidence: vec![CitationId("fact".into())],
            description: Review::Judged {
                judgment: RouteDescription {
                    role,
                    other_roles: vec![],
                },
                reason: "Because.".into(),
                primary: CitationId("fact".into()),
                supporting: vec![],
            },
        },
    )]);
    map.routes = BTreeMap::new();
    map
}

#[test]
fn each_view_groups_by_what_it_shows() {
    let grouped = BTreeMap::from([("port".to_string(), vec!["adapter".to_string()])]);
    let none = BTreeMap::new();
    let groups = |map: &ProjectMap| {
        let view = view(map);
        [
            view.groups.current,
            view.groups.planned,
            view.groups.changes,
        ]
    };
    // An update turns "implements" into direct use: only Current groups them.
    let update = Status::ProposedUpdate {
        proposed: Review::Judged {
            judgment: RouteDescription {
                role: UseRole::DirectUse,
                other_roles: vec![],
            },
            reason: "No longer implements it.".into(),
            primary: CitationId("fact".into()),
            supporting: vec![],
        },
    };
    assert_eq!(
        groups(&port_and_adapter(UseRole::Implements, update)),
        [grouped.clone(), none.clone(), none.clone()]
    );
    // A removal: Planned leaves the dependency out; Changes still draws it.
    assert_eq!(
        groups(&port_and_adapter(
            UseRole::Implements,
            Status::ProposedRemoval
        )),
        [grouped.clone(), none.clone(), grouped.clone()]
    );
    // An addition: Current leaves it out.
    assert_eq!(
        groups(&port_and_adapter(
            UseRole::Implements,
            Status::ProposedAddition
        )),
        [none, grouped.clone(), grouped]
    );
}

/// A route from an item in `origin` with one hop into each listed module,
/// each receiver using its sender.
fn route_from(origin: &str, modules: &[&str]) -> MapRoute {
    let mut route = route_through(modules, Review::Unreviewed);
    route.origin = RoutePoint::Project(ProjectPoint {
        module: ModuleId(origin.into()),
        item: Some("Thing".into()),
    });
    for hop in &mut route.hops {
        if let HopTarget::Project { link, .. } = &mut hop.to {
            *link = HopLink::ReceiverUsesSender;
        }
    }
    route
}

#[test]
fn an_arrow_lists_the_one_hop_routes_that_travel_along_it() {
    let mut map = mixed_map();
    map.routes = BTreeMap::from([
        // `new` uses `core`: the arrow new → core.
        (RouteId("direct-1".into()), route_from("core", &["new"])),
        (RouteId("direct-2".into()), route_from("core", &["new"])),
        // Two hops: listed among the routes, not under the arrow.
        (
            RouteId("longer".into()),
            route_from("core", &["new", "stray"]),
        ),
        // One hop along a dependency the map does not have.
        (RouteId("orphan".into()), route_from("stray", &["core"])),
    ]);
    let view = view(&map);
    let edge = view
        .edges
        .iter()
        .find(|e| e.from == "new" && e.to == "core")
        .unwrap();
    assert_eq!(edge.routes, ["direct-1", "direct-2"]);
    assert!(
        view.edges
            .iter()
            .filter(|e| !(e.from == "new" && e.to == "core"))
            .all(|e| e.routes.is_empty())
    );
    assert_eq!(view.routes.len(), 4, "every route stays in the view");
}
