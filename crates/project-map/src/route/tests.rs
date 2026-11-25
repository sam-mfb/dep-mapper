use crate::*;

fn project(module: &str, link: HopLink) -> HopTarget {
    HopTarget::Project {
        point: ProjectPoint {
            module: ModuleId(module.into()),
            item: None,
        },
        link,
    }
}

fn hop(to: HopTarget) -> MapHop {
    MapHop {
        to,
        evidence: CitationId("fact".into()),
        support: HopSupport::Established,
        role: None,
    }
}

fn dependency(from: &str, to: &str) -> Option<DependencyId> {
    Some(DependencyId {
        from: ModuleId(from.into()),
        to: ModuleId(to.into()),
    })
}

/// supplier -> program -> receiver -> receiver -> tokio -> receiver
fn route() -> MapRoute {
    MapRoute {
        status: Status::Observed,
        use_site: None,
        origin: RoutePoint::Project(ProjectPoint {
            module: ModuleId("supplier".into()),
            item: Some("Store".into()),
        }),
        hops: vec![
            hop(project("program", HopLink::ReceiverUsesSender)),
            hop(project("receiver", HopLink::SenderUsesReceiver)),
            hop(project("receiver", HopLink::SameModule)),
            hop(HopTarget::Outside(OutsidePoint::External("tokio".into()))),
            hop(project("receiver", HopLink::FromOutside)),
        ],
        end: MapRouteEnd::ReachedUse,
        description: Review::Unreviewed,
    }
}

#[test]
fn a_hop_into_a_user_travels_along_its_dependency_on_the_sender() {
    assert_eq!(route().hop_dependency(0), dependency("program", "supplier"));
}

#[test]
fn a_hop_out_of_a_user_travels_along_its_dependency_on_the_receiver() {
    assert_eq!(route().hop_dependency(1), dependency("program", "receiver"));
}

#[test]
fn hops_within_a_module_or_across_the_project_edge_have_none() {
    let route = route();
    for index in 2..=5 {
        assert_eq!(route.hop_dependency(index), None, "hop {index}");
    }
}

#[test]
fn a_link_naming_a_sender_outside_the_project_has_none() {
    let mut route = route();
    route.hops[4].to = project("receiver", HopLink::ReceiverUsesSender);
    assert_eq!(route.hop_dependency(4), None);
}

#[test]
fn a_hop_target_keeps_its_point() {
    let target = project("receiver", HopLink::SameModule);
    assert_eq!(target.module(), Some(&ModuleId("receiver".into())));
    assert_eq!(
        target.point(),
        RoutePoint::Project(ProjectPoint {
            module: ModuleId("receiver".into()),
            item: None,
        })
    );
}
