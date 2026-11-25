use super::*;
use crate::{Build, Endpoint, FactKind, Hop, ModuleId, RouteEnd, Support, UnitId};
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
        facts: BTreeMap::from([(
            FactId(shared.into()),
            Fact {
                unit: UnitId("unit".into()),
                subject: Endpoint::Module(user.clone()),
                kind: FactKind::Construct,
                object: Endpoint::Item(ItemId(shared.into())),
                site: None,
            },
        )]),
        routes: BTreeMap::from([(
            RouteId(shared.into()),
            Route {
                use_site: Some(FactId(shared.into())),
                origin: Endpoint::Module(used),
                hops: vec![Hop {
                    to: Endpoint::Module(user),
                    fact: FactId(shared.into()),
                    support: Support::Established,
                }],
                end: RouteEnd::ReachedUse,
            },
        )]),
        gaps: vec![],
    }
}

#[test]
fn a_reference_resolves_to_what_it_names_even_when_ids_share_text() {
    let graph = graph();
    let shared = "shared";
    let route = GraphRef::from(RouteId(shared.into()));
    let fact = GraphRef::from(FactId(shared.into()));
    let item = GraphRef::from(ItemId(shared.into()));
    assert_ne!(route, fact);
    assert_ne!(fact, item);
    assert!(matches!(
        graph.resolve(&route),
        Some(Cited::Route(found)) if found.end == RouteEnd::ReachedUse
    ));
    assert!(matches!(
        graph.resolve(&fact),
        Some(Cited::Fact(found)) if found.kind == FactKind::Construct
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
