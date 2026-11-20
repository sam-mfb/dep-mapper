//! An offline consumer can inspect a reviewed route using only the map contract.
use project_map::*;
use std::collections::BTreeMap;

fn endpoint(module: &str, item: &str) -> MapEndpoint {
    MapEndpoint::Project {
        module: module.into(),
        item: Some(item.into()),
    }
}

#[test]
fn follow_a_reviewed_handoff_to_source_and_distinguish_a_deferred_route() {
    let supply = CitationId("kkt:backup field".into());
    let field = endpoint("kashkeshet", "NativeParts::install_parts / backups");
    let used = CitationId("kkt:backup call".into());
    let route = MapRoute {
        origin: endpoint("kkt-backup-os", "OsBackups"),
        use_site: Some(used.clone()),
        hops: vec![
            MapHop {
                to: field,
                evidence: supply.clone(),
                support: HopSupport::Established,
                role: Some(UseRole::Wires),
            },
            MapHop {
                to: endpoint("kkt-install-commands", "local_backup"),
                evidence: used.clone(),
                support: HopSupport::Established,
                role: Some(UseRole::DirectUse),
            },
        ],
        end: MapRouteEnd::ReachedUse,
        review: Review::Judged {
            description: RouteDescription {
                role: UseRole::FullInjection,
                other_roles: vec![],
            },
            reason: "NativeParts fills the install command’s backup dependency.".into(),
            primary: supply.clone(),
            supporting: vec![used.clone()],
        },
    };
    let another = MapRoute {
        origin: endpoint("kkt-backup-os", "OsBackups"),
        use_site: None,
        hops: vec![],
        end: MapRouteEnd::Stopped {
            at: supply.clone(),
            reason: "Untraced beyond the handoff.".into(),
        },
        review: Review::Deferred {
            reason: "Needs review.".into(),
        },
    };
    let modules = ["kkt-backup-os", "kashkeshet", "kkt-install-commands"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                MapModule {
                    name: name.into(),
                    surface: vec![],
                    review: Review::Unreviewed,
                },
            )
        })
        .collect();
    let map = ProjectMap {
        graph_id: "kkt-backup/linux:fixture".into(),
        project: "kashkeshet".into(),
        source_revision: "41fbc289".into(),
        target: "kkt".into(),
        configuration: "default features".into(),
        layers: vec![],
        modules,
        routes: BTreeMap::from([("supply".into(), route), ("open".into(), another)]),
        evidence: BTreeMap::from([
            (
                supply,
                MapEvidence {
                    summary: "NativeParts supplies the BACKUPS static.".into(),
                    build_unit: Some("kkt".into()),
                    location: Some(Location {
                        file: "src/machine/native.rs".into(),
                        bytes: 4270..4287,
                    }),
                },
            ),
            (
                used.clone(),
                MapEvidence {
                    summary: "local_backup calls write through parts.backups.".into(),
                    build_unit: Some("kkt-install-commands/lib".into()),
                    location: Some(Location {
                        file: "crates/kkt-install-commands/src/lifecycle.rs".into(),
                        bytes: 17279..17315,
                    }),
                },
            ),
        ]),
        coverage_gaps: vec![],
    };
    let Review::Judged {
        primary,
        description,
        ..
    } = &map.routes["supply"].review
    else {
        panic!("missing review")
    };
    assert_eq!(description.role, UseRole::FullInjection);
    let call = &map.routes["supply"].hops[1];
    assert_eq!(Some(&call.evidence), map.routes["supply"].use_site.as_ref());
    let location = map.evidence[&call.evidence].location.as_ref().unwrap();
    assert_eq!(
        location.file,
        "crates/kkt-install-commands/src/lifecycle.rs"
    );
    assert_eq!(map.evidence[primary].build_unit.as_deref(), Some("kkt"));
    assert_eq!(map.routes["supply"].hops[0].evidence, *primary);
    assert!(matches!(
        map.routes["open"].end,
        MapRouteEnd::Stopped { .. }
    ));
    assert!(matches!(map.routes["open"].review, Review::Deferred { .. }));
    assert!(matches!(
        map.modules["kashkeshet"].review,
        Review::Unreviewed
    ));
}
