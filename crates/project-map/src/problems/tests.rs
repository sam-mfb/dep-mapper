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

fn module(status: Status) -> MapModule {
    MapModule {
        status,
        name: "ignored".into(),
        surface: vec![],
        description: Review::Unreviewed,
    }
}

fn dependency(status: Status, citation: &str) -> MapDependency {
    MapDependency {
        status,
        evidence: vec![cite(citation)],
        description: Review::Unreviewed,
    }
}

/// A route from `origin` to `receiver`, which uses `origin`.
fn route(status: Status, origin: &str, receiver: &str, citation: &str) -> MapRoute {
    let support = match status {
        Status::Proposed => HopSupport::Proposed,
        Status::Observed | Status::ProposedRemoval => HopSupport::Established,
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
        description: Review::Judged {
            judgment: RouteDescription {
                role: UseRole::DirectUse,
                other_roles: vec![],
            },
            reason: "Because.".into(),
            primary: cite(citation),
            supporting: vec![],
        },
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

/// Observed modules `a` and `b`, proposed `p`, and `r` proposed for removal,
/// with a dependency and a route of each status. `src` is source evidence
/// and `note` a proposal note.
fn mixed() -> ProjectMap {
    let mut a = module(Status::Observed);
    a.description = Review::Judged {
        judgment: ModuleDescription {
            concern: "A concern.".into(),
            kind: ModuleKind::Logic,
            layer: Some(LayerName("Core".into())),
        },
        reason: "Because.".into(),
        primary: cite("src"),
        supporting: vec![],
    };
    ProjectMap {
        project: "fixture".into(),
        source: None,
        layers: Layers::new(vec![LayerName("Core".into())]).unwrap(),
        modules: BTreeMap::from([
            (id("a"), a),
            (id("b"), module(Status::Observed)),
            (id("p"), module(Status::Proposed)),
            (id("r"), module(Status::ProposedRemoval)),
        ]),
        dependencies: BTreeMap::from([
            (dep("a", "b"), dependency(Status::Observed, "src")),
            (dep("p", "a"), dependency(Status::Proposed, "note")),
            (dep("r", "b"), dependency(Status::ProposedRemoval, "src")),
        ]),
        routes: BTreeMap::from([
            (
                RouteId("seen".into()),
                route(Status::Observed, "b", "a", "src"),
            ),
            (
                RouteId("planned".into()),
                route(Status::Proposed, "a", "p", "note"),
            ),
            (
                RouteId("going".into()),
                route(Status::ProposedRemoval, "b", "r", "src"),
            ),
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
fn a_map_mixing_observed_proposed_and_removed_parts_has_no_problems() {
    assert_eq!(mixed().problems(), []);
}

#[test]
fn a_citation_missing_from_the_evidence_is_reported() {
    let mut map = mixed();
    map.evidence.remove(&cite("note"));
    assert_eq!(
        map.problems(),
        [
            MapProblem::MissingEvidence {
                part: MapPart::Dependency(dep("p", "a")),
                citation: cite("note"),
            },
            MapProblem::MissingEvidence {
                part: route_part("planned"),
                citation: cite("note"),
            },
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
fn an_observed_part_resting_on_a_proposed_part_is_reported() {
    let mut map = mixed();
    map.dependencies
        .insert(dep("b", "p"), dependency(Status::Observed, "src"));
    assert_eq!(
        map.problems(),
        [MapProblem::ObservedRestsOnProposed {
            part: MapPart::Dependency(dep("b", "p")),
            proposed: MapPart::Module(id("p")),
        }]
    );
}

#[test]
fn a_route_in_code_resting_on_a_proposed_dependency_is_reported() {
    let mut map = mixed();
    map.routes.insert(
        RouteId("claimed".into()),
        route(Status::Observed, "a", "p", "src"),
    );
    assert_eq!(
        map.problems(),
        [
            MapProblem::ObservedRestsOnProposed {
                part: route_part("claimed"),
                proposed: MapPart::Module(id("p")),
            },
            MapProblem::ObservedRestsOnProposed {
                part: route_part("claimed"),
                proposed: MapPart::Dependency(dep("p", "a")),
            },
        ]
    );
}

#[test]
fn parts_in_code_citing_a_proposal_note_are_reported() {
    let mut map = mixed();
    map.routes.get_mut(&RouteId("seen".into())).unwrap().hops[0].evidence = cite("note");
    map.dependencies.get_mut(&dep("r", "b")).unwrap().evidence = vec![cite("note")];
    assert_eq!(
        map.problems(),
        [
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
fn a_proposed_part_resting_on_a_removal_is_reported() {
    let mut map = mixed();
    map.dependencies
        .insert(dep("p", "r"), dependency(Status::Proposed, "note"));
    assert_eq!(
        map.problems(),
        [MapProblem::ProposedRestsOnRemoval {
            part: MapPart::Dependency(dep("p", "r")),
            removed: MapPart::Module(id("r")),
        }]
    );
}

#[test]
fn a_module_in_an_unlisted_layer_is_reported() {
    let mut map = mixed();
    map.layers = Layers::new(vec![LayerName("Edge".into())]).unwrap();
    assert_eq!(
        map.problems(),
        [MapProblem::UnlistedLayer {
            module: id("a"),
            layer: LayerName("Core".into()),
        }]
    );
}
