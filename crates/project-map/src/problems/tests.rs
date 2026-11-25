use super::*;
use crate::{
    EvidenceKind, HopLink, HopTarget, Layers, MapDependency, MapEvidence, MapHop, MapModule,
    ModuleDescription, ModuleKind, ProjectPoint, RouteDescription, RoutePoint, UseRole,
};
use std::collections::BTreeMap;

fn id(name: &str) -> ModuleId {
    ModuleId(name.into())
}

fn dep(from: &str, to: &str) -> DependencyId {
    DependencyId {
        from: id(from),
        to: id(to),
    }
}

fn cite(name: &str) -> CitationId {
    CitationId(name.into())
}

fn module_review(kind: ModuleKind, citation: &str) -> Review<ModuleDescription> {
    Review::Judged {
        judgment: ModuleDescription {
            concern: "A concern.".into(),
            kind,
            layer: Some(LayerName("Core".into())),
        },
        reason: "Because.".into(),
        primary: cite(citation),
        supporting: vec![],
    }
}

fn route_review(role: UseRole, citation: &str) -> Review<RouteDescription> {
    Review::Judged {
        judgment: RouteDescription {
            role,
            other_roles: vec![],
        },
        reason: "Because.".into(),
        primary: cite(citation),
        supporting: vec![],
    }
}

fn module(status: Status<ModuleDescription>) -> MapModule {
    MapModule {
        status,
        name: "ignored".into(),
        surface: vec![],
        description: Review::Unreviewed,
    }
}

fn dependency(status: Status<RouteDescription>, citation: &str) -> MapDependency {
    MapDependency {
        status,
        evidence: vec![cite(citation)],
        description: Review::Unreviewed,
    }
}

/// A route from `origin` to `receiver`, which uses `origin`.
fn route(
    status: Status<RouteDescription>,
    origin: &str,
    receiver: &str,
    citation: &str,
) -> MapRoute {
    let support = if status.is_addition() {
        HopSupport::Proposed
    } else {
        HopSupport::Established
    };
    MapRoute {
        status,
        use_site: None,
        origin: RoutePoint::Project(ProjectPoint {
            module: id(origin),
            item: None,
        }),
        hops: vec![MapHop {
            to: HopTarget::Project {
                point: ProjectPoint {
                    module: id(receiver),
                    item: None,
                },
                link: HopLink::ReceiverUsesSender,
            },
            evidence: cite(citation),
            support,
            role: None,
        }],
        end: MapRouteEnd::ReachedUse,
        description: route_review(UseRole::DirectUse, citation),
    }
}

fn evidence(kind: EvidenceKind) -> MapEvidence {
    MapEvidence {
        kind,
        build_unit: None,
        summary: "A summary.".into(),
        location: None,
    }
}

/// Observed modules `a` and `b`, an added `p`, an updated `u` and `r`
/// proposed for removal, with a dependency and a route of each status.
/// `src` is source evidence and `note` a proposal note.
fn mixed() -> ProjectMap {
    let mut a = module(Status::Observed);
    a.description = module_review(ModuleKind::Logic, "src");
    let mut u = module(Status::ProposedUpdate {
        proposed: module_review(ModuleKind::Effect, "note"),
    });
    u.description = module_review(ModuleKind::Logic, "src");
    let mut updated = dependency(
        Status::ProposedUpdate {
            proposed: route_review(UseRole::Wires, "note"),
        },
        "src",
    );
    updated.description = route_review(UseRole::DirectUse, "src");
    let mut changed = route(
        Status::ProposedUpdate {
            proposed: route_review(UseRole::Incidental, "note"),
        },
        "b",
        "u",
        "src",
    );
    changed.description = route_review(UseRole::DirectUse, "src");
    ProjectMap {
        project: "fixture".into(),
        source: None,
        layers: Layers::new(vec![LayerName("Core".into())]).unwrap(),
        modules: BTreeMap::from([
            (id("a"), a),
            (id("b"), module(Status::Observed)),
            (id("p"), module(Status::ProposedAddition)),
            (id("r"), module(Status::ProposedRemoval)),
            (id("u"), u),
        ]),
        dependencies: BTreeMap::from([
            (dep("a", "b"), dependency(Status::Observed, "src")),
            (dep("p", "a"), dependency(Status::ProposedAddition, "note")),
            (dep("r", "b"), dependency(Status::ProposedRemoval, "src")),
            (dep("u", "b"), updated),
        ]),
        routes: BTreeMap::from([
            (
                RouteId("seen".into()),
                route(Status::Observed, "b", "a", "src"),
            ),
            (
                RouteId("planned".into()),
                route(Status::ProposedAddition, "a", "p", "note"),
            ),
            (
                RouteId("going".into()),
                route(Status::ProposedRemoval, "b", "r", "src"),
            ),
            (RouteId("changed".into()), changed),
        ]),
        evidence: BTreeMap::from([
            (cite("src"), evidence(EvidenceKind::Source)),
            (cite("note"), evidence(EvidenceKind::Proposal)),
        ]),
        coverage_gaps: vec![],
    }
}

fn route_part(name: &str) -> MapPart {
    MapPart::Route(RouteId(name.into()))
}

#[test]
fn a_map_mixing_every_status_has_no_problems() {
    assert_eq!(mixed().problems(), []);
}

#[test]
fn a_citation_missing_from_the_evidence_is_reported() {
    let mut map = mixed();
    map.evidence.remove(&cite("note"));
    let parts: Vec<MapPart> = map
        .problems()
        .into_iter()
        .map(|problem| match problem {
            MapProblem::MissingEvidence { part, citation } if citation == cite("note") => part,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        parts,
        [
            MapPart::Module(id("u")),
            MapPart::Dependency(dep("p", "a")),
            MapPart::Dependency(dep("u", "b")),
            route_part("changed"),
            route_part("planned"),
        ]
    );
}

#[test]
fn a_dependency_on_a_module_not_on_the_map_is_reported() {
    let mut map = mixed();
    map.dependencies
        .insert(dep("a", "ghost"), dependency(Status::Observed, "src"));
    assert_eq!(
        map.problems(),
        [MapProblem::MissingModule {
            part: MapPart::Dependency(dep("a", "ghost")),
            module: id("ghost"),
        }]
    );
}

#[test]
fn a_hop_along_a_dependency_not_on_the_map_is_reported() {
    let mut map = mixed();
    map.dependencies.remove(&dep("a", "b"));
    assert_eq!(
        map.problems(),
        [MapProblem::MissingDependency {
            route: RouteId("seen".into()),
            hop: 0,
            dependency: dep("a", "b"),
        }]
    );
}

#[test]
fn a_part_in_code_resting_on_an_addition_is_reported() {
    let mut map = mixed();
    map.dependencies
        .insert(dep("b", "p"), dependency(Status::Observed, "src"));
    map.dependencies.insert(
        dep("u", "p"),
        dependency(
            Status::ProposedUpdate {
                proposed: route_review(UseRole::Wires, "note"),
            },
            "src",
        ),
    );
    assert_eq!(
        map.problems(),
        [
            MapProblem::ObservedRestsOnAddition {
                part: MapPart::Dependency(dep("b", "p")),
                addition: MapPart::Module(id("p")),
            },
            MapProblem::ObservedRestsOnAddition {
                part: MapPart::Dependency(dep("u", "p")),
                addition: MapPart::Module(id("p")),
            },
        ]
    );
}

#[test]
fn a_route_in_code_resting_on_an_added_dependency_is_reported() {
    let mut map = mixed();
    map.routes.insert(
        RouteId("claimed".into()),
        route(Status::Observed, "a", "p", "src"),
    );
    assert_eq!(
        map.problems(),
        [
            MapProblem::ObservedRestsOnAddition {
                part: route_part("claimed"),
                addition: MapPart::Module(id("p")),
            },
            MapProblem::ObservedRestsOnAddition {
                part: route_part("claimed"),
                addition: MapPart::Dependency(dep("p", "a")),
            },
        ]
    );
}

#[test]
fn evidence_and_reviews_in_code_citing_a_note_are_reported() {
    let mut map = mixed();
    map.routes.get_mut(&RouteId("seen".into())).unwrap().hops[0].evidence = cite("note");
    map.dependencies.get_mut(&dep("r", "b")).unwrap().evidence = vec![cite("note")];
    // The review an updated module has in code cannot cite a note either.
    map.modules.get_mut(&id("u")).unwrap().description = module_review(ModuleKind::Logic, "note");
    assert_eq!(
        map.problems(),
        [
            MapProblem::ObservedCitesProposal {
                part: MapPart::Module(id("u")),
                citation: cite("note"),
            },
            MapProblem::ObservedCitesProposal {
                part: MapPart::Dependency(dep("r", "b")),
                citation: cite("note"),
            },
            MapProblem::ObservedCitesProposal {
                part: route_part("seen"),
                citation: cite("note"),
            },
        ]
    );
}

#[test]
fn a_hop_whose_support_does_not_match_its_route_is_reported() {
    let mut map = mixed();
    map.routes.get_mut(&RouteId("seen".into())).unwrap().hops[0].support = HopSupport::Proposed;
    map.routes.get_mut(&RouteId("planned".into())).unwrap().hops[0].support =
        HopSupport::Possible { condition: None };
    assert_eq!(
        map.problems(),
        [
            MapProblem::HopStatusMismatch {
                route: RouteId("planned".into()),
                hop: 0,
            },
            MapProblem::HopStatusMismatch {
                route: RouteId("seen".into()),
                hop: 0,
            },
        ]
    );
}

#[test]
fn additions_and_updates_resting_on_a_removal_are_reported() {
    let mut map = mixed();
    map.dependencies
        .insert(dep("p", "r"), dependency(Status::ProposedAddition, "note"));
    let mut updated = dependency(
        Status::ProposedUpdate {
            proposed: route_review(UseRole::Wires, "note"),
        },
        "src",
    );
    updated.description = route_review(UseRole::DirectUse, "src");
    map.dependencies.insert(dep("u", "r"), updated);
    assert_eq!(
        map.problems(),
        [
            MapProblem::ProposedRestsOnRemoval {
                part: MapPart::Dependency(dep("p", "r")),
                removed: MapPart::Module(id("r")),
            },
            MapProblem::ProposedRestsOnRemoval {
                part: MapPart::Dependency(dep("u", "r")),
                removed: MapPart::Module(id("r")),
            },
        ]
    );
}

#[test]
fn an_update_reaching_the_same_decision_is_reported() {
    let mut map = mixed();
    // Only the reason and citations differ from the review in code.
    map.modules.get_mut(&id("u")).unwrap().status = Status::ProposedUpdate {
        proposed: module_review(ModuleKind::Logic, "note"),
    };
    map.routes
        .get_mut(&RouteId("changed".into()))
        .unwrap()
        .status = Status::ProposedUpdate {
        proposed: route_review(UseRole::DirectUse, "note"),
    };
    assert_eq!(
        map.problems(),
        [
            MapProblem::UpdateChangesNothing {
                part: MapPart::Module(id("u")),
            },
            MapProblem::UpdateChangesNothing {
                part: route_part("changed"),
            },
        ]
    );
}

#[test]
fn a_module_in_an_unlisted_layer_is_reported_for_either_review() {
    let mut map = mixed();
    let mut moved = module_review(ModuleKind::Effect, "note");
    if let Review::Judged { judgment, .. } = &mut moved {
        judgment.layer = Some(LayerName("Edge".into()));
    }
    map.modules.get_mut(&id("u")).unwrap().status = Status::ProposedUpdate { proposed: moved };
    assert_eq!(
        map.problems(),
        [MapProblem::UnlistedLayer {
            module: id("u"),
            layer: LayerName("Edge".into()),
        }]
    );
    map.layers = Layers::new(vec![LayerName("Edge".into())]).unwrap();
    let unlisted: Vec<ModuleId> = map
        .problems()
        .into_iter()
        .map(|problem| match problem {
            MapProblem::UnlistedLayer { module, .. } => module,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(unlisted, [id("a"), id("u")]);
}
