use super::{PROPOSED_ROUTE, sharsheret};
use crate::*;

#[test]
fn records_every_module_dependency_and_route_of_the_proposal() {
    let map = sharsheret();
    assert_eq!(map.modules.len(), 11);
    assert_eq!(map.dependencies.len(), 20);
    assert_eq!(map.routes.len(), 11);
}

#[test]
fn the_parts_of_the_example_fit_together() {
    assert_eq!(sharsheret().problems(), []);
}

#[test]
fn one_route_is_proposed_and_everything_else_is_observed() {
    let map = sharsheret();
    assert!(map.modules.values().all(|m| m.status == Status::Observed));
    assert!(
        map.dependencies
            .values()
            .all(|d| d.status == Status::Observed)
    );
    let proposed: Vec<&str> = map
        .routes
        .iter()
        .filter(|(_, route)| route.status == Status::ProposedAddition)
        .map(|(id, _)| id.0.as_str())
        .collect();
    assert_eq!(proposed, [PROPOSED_ROUTE]);
    let route = &map.routes[&RouteId(PROPOSED_ROUTE.into())];
    let Review::Judged { primary, .. } = &route.description else {
        panic!("the proposed route is judged")
    };
    assert_eq!(map.evidence[primary].kind, EvidenceKind::Proposal);
}

#[test]
fn every_module_is_judged_with_a_layer() {
    for module in sharsheret().modules.values() {
        let Review::Judged { judgment, .. } = &module.description else {
            panic!("{} is not judged", module.name)
        };
        assert!(judgment.layer.is_some(), "{} has no layer", module.name);
    }
}

#[test]
fn every_hop_travels_along_a_dependency() {
    // problems() checks the dependencies hops name; this checks each hop names one.
    for (id, route) in &sharsheret().routes {
        for index in 0..route.hops.len() {
            assert!(
                route.hop_dependency(index).is_some(),
                "{id:?} hop {index} names no dependency"
            );
        }
    }
}
