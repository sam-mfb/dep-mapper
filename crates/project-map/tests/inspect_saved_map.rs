//! An offline consumer can inspect a reviewed route using only the map contract.
use project_map::*;
use std::collections::BTreeMap;

fn origin(module: &str, item: &str) -> RoutePoint {
    RoutePoint::Project(ProjectPoint {
        module: ModuleId(module.into()),
        item: Some(item.into()),
    })
}

fn target(module: &str, item: &str, link: HopLink) -> HopTarget {
    HopTarget::Project {
        point: ProjectPoint {
            module: ModuleId(module.into()),
            item: Some(item.into()),
        },
        link,
    }
}

#[test]
fn follow_a_reviewed_handoff_to_source_and_distinguish_a_deferred_route() {
    let supply = CitationId("kkt:backup field".into());
    let field = target(
        "kashkeshet",
        "NativeParts::install_parts / backups",
        HopLink::ReceiverUsesSender,
    );
    let used = CitationId("kkt:backup call".into());
    let route = MapRoute {
        status: Status::Observed,
        origin: origin("kkt-backup-os", "OsBackups"),
        use_site: Some(used.clone()),
        hops: vec![
            MapHop {
                to: field,
                evidence: supply.clone(),
                support: HopSupport::Established,
                role: Some(UseRole::Wires),
            },
            MapHop {
                to: target(
                    "kkt-install-commands",
                    "local_backup",
                    HopLink::SenderUsesReceiver,
                ),
                evidence: used.clone(),
                support: HopSupport::Established,
                role: Some(UseRole::DirectUse),
            },
        ],
        end: MapRouteEnd::ReachedUse,
        description: Review::Judged {
            judgment: RouteDescription {
                role: UseRole::FullInjection,
                other_roles: vec![],
            },
            reason: "NativeParts fills the install command’s backup dependency.".into(),
            primary: supply.clone(),
            supporting: vec![used.clone()],
        },
    };
    let another = MapRoute {
        status: Status::Observed,
        origin: origin("kkt-backup-os", "OsBackups"),
        use_site: None,
        hops: vec![],
        end: MapRouteEnd::Stopped {
            at: supply.clone(),
            reason: "Untraced beyond the handoff.".into(),
        },
        description: Review::Deferred {
            reason: "Needs review.".into(),
        },
    };
    let modules = ["kkt-backup-os", "kashkeshet", "kkt-install-commands"]
        .into_iter()
        .map(|name| {
            (
                ModuleId(name.into()),
                MapModule {
                    status: Status::Observed,
                    name: name.into(),
                    surface: vec![],
                    description: Review::Unreviewed,
                },
            )
        })
        .collect();
    let map = ProjectMap {
        project: "kashkeshet".into(),
        source: Some(MapSource {
            graph_id: GraphId("kkt-backup/linux:fixture".into()),
            source_revision: SourceRevision("41fbc289".into()),
            target: "kkt".into(),
            configuration: "default features".into(),
        }),
        layers: Layers::empty(),
        modules,
        dependencies: BTreeMap::new(),
        routes: BTreeMap::from([
            (RouteId("supply".into()), route),
            (RouteId("open".into()), another),
        ]),
        evidence: BTreeMap::from([
            (
                supply,
                MapEvidence {
                    kind: EvidenceKind::Source,
                    summary: "NativeParts supplies the BACKUPS static.".into(),
                    build_unit: Some(UnitId("kkt".into())),
                    location: Some(Location {
                        file: SourcePath("src/machine/native.rs".into()),
                        bytes: 4270..4287,
                    }),
                },
            ),
            (
                used.clone(),
                MapEvidence {
                    kind: EvidenceKind::Source,
                    summary: "local_backup calls write through parts.backups.".into(),
                    build_unit: Some(UnitId("kkt-install-commands/lib".into())),
                    location: Some(Location {
                        file: SourcePath("crates/kkt-install-commands/src/lifecycle.rs".into()),
                        bytes: 17279..17315,
                    }),
                },
            ),
        ]),
        coverage_gaps: vec![],
    };
    let reviewed = &map.routes[&RouteId("supply".into())];
    let deferred = &map.routes[&RouteId("open".into())];
    let Review::Judged {
        primary, judgment, ..
    } = &reviewed.description
    else {
        panic!("missing review")
    };
    assert_eq!(judgment.role, UseRole::FullInjection);
    let call = &reviewed.hops[1];
    assert_eq!(Some(&call.evidence), reviewed.use_site.as_ref());
    let location = map.evidence[&call.evidence].location.as_ref().unwrap();
    assert_eq!(
        location.file,
        SourcePath("crates/kkt-install-commands/src/lifecycle.rs".into())
    );
    assert_eq!(map.evidence[primary].build_unit, Some(UnitId("kkt".into())));
    assert_eq!(reviewed.hops[0].evidence, *primary);
    assert!(matches!(deferred.end, MapRouteEnd::Stopped { .. }));
    assert!(matches!(deferred.description, Review::Deferred { .. }));
    assert!(matches!(
        map.modules[&ModuleId("kashkeshet".into())].description,
        Review::Unreviewed
    ));
}
