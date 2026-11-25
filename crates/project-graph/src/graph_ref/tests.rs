use super::*;
use crate::{Build, Endpoint, Fact, ModuleId, UnitId};
use std::collections::BTreeMap;

fn graph() -> ProjectGraph {
    let user = ModuleId("user".into());
    let used = ModuleId("used".into());
    let shared = "shared";
    ProjectGraph {
        id: crate::GraphId("fixture".into()),
        build: Build {
            project: "fixture".into(),
            source_revision: "0".into(),
            target: "fixture".into(),
            configuration: "default".into(),
        },
        modules: BTreeMap::new(),
        units: BTreeMap::new(),
        items: BTreeMap::from([(
            ItemId(shared.into()),
            Item {
                module: used.clone(),
                name: "Thing".into(),
                kind: crate::ItemKind::Type,
                visibility: crate::Visibility::Exported,
                signature: None,
                declaration: None,
            },
        )]),
        evidence: BTreeMap::from([(
            EvidenceId(shared.into()),
            Evidence {
                unit: UnitId("unit".into()),
                subject: Endpoint::Module(user.clone()),
                kind: Fact::Construct,
                object: Endpoint::Item(ItemId(shared.into())),
                site: None,
            },
        )]),
        routes: BTreeMap::new(),
        connections: BTreeMap::from([(
            ConnectionId(shared.into()),
            Connection {
                user,
                used,
                evidence: vec![EvidenceId(shared.into())],
            },
        )]),
        gaps: vec![],
    }
}

#[test]
fn a_reference_resolves_to_what_it_names_even_when_ids_share_text() {
    let graph = graph();
    let shared = "shared";
    let connection = GraphRef::from(ConnectionId(shared.into()));
    let evidence = GraphRef::from(EvidenceId(shared.into()));
    let item = GraphRef::from(ItemId(shared.into()));
    assert_ne!(connection, evidence);
    assert_ne!(evidence, item);
    assert!(matches!(
        graph.resolve(&connection),
        Some(Cited::Connection(found)) if found.user == ModuleId("user".into())
    ));
    assert!(matches!(
        graph.resolve(&evidence),
        Some(Cited::Evidence(found)) if found.kind == Fact::Construct
    ));
    assert!(matches!(
        graph.resolve(&item),
        Some(Cited::Item(found)) if found.name == "Thing"
    ));
}

#[test]
fn a_reference_to_something_missing_resolves_to_nothing() {
    let graph = graph();
    assert_eq!(graph.resolve(&GraphRef::from(ItemId("gone".into()))), None);
}
