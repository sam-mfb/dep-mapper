//! A small graph for tests: `app` builds a `DiskStore` from `disk`, which
//! implements the `Store` contract in `store`, and `app` also writes a file
//! through the standard library.

use project_graph::*;
use std::collections::BTreeMap;

pub fn graph() -> ProjectGraph {
    let module = |name: &str| {
        (
            ModuleId(name.into()),
            Module {
                name: name.into(),
                root: format!("crates/{name}"),
            },
        )
    };
    let unit = |name: &str| {
        (
            UnitId(format!("{name}/lib")),
            BuildUnit {
                module: ModuleId(name.into()),
                target: "lib".into(),
                settings: BTreeMap::new(),
            },
        )
    };
    let item = |id: &str, module: &str, name: &str, kind: ItemKind| {
        (
            ItemId(id.into()),
            Item {
                module: ModuleId(module.into()),
                name: name.into(),
                kind,
                visibility: Visibility::Exported,
                signature: None,
                declaration: Some(SourceSpan {
                    file: format!("crates/{module}/src/lib.rs"),
                    bytes: 10..20,
                }),
            },
        )
    };
    let fact = |id: &str, unit: &str, subject: Endpoint, kind: FactKind, object: Endpoint| {
        (
            FactId(id.into()),
            Fact {
                unit: UnitId(format!("{unit}/lib")),
                subject,
                kind,
                object,
                site: Some(SourceSpan {
                    file: format!("crates/{unit}/src/lib.rs"),
                    bytes: 30..40,
                }),
            },
        )
    };
    let it = |id: &str| Endpoint::Item(ItemId(id.into()));
    let route = |origin: Endpoint, hops: Vec<(Endpoint, &str)>| Route {
        use_sites: Vec::new(),
        origin,
        hops: hops
            .into_iter()
            .map(|(to, fact)| Hop {
                to,
                facts: vec![FactId(fact.into())],
                support: Support::Established,
            })
            .collect(),
        end: RouteEnd::ReachedUse,
    };
    ProjectGraph {
        id: GraphId("fixture@1".into()),
        build: Build {
            project: "fixture".into(),
            source_revision: "abc".into(),
            target: "x86_64".into(),
            configuration: "default".into(),
        },
        modules: BTreeMap::from([module("app"), module("disk"), module("store")]),
        units: BTreeMap::from([unit("app"), unit("disk"), unit("store")]),
        items: BTreeMap::from([
            item("store::Store", "store", "Store", ItemKind::Type),
            item("disk::DiskStore", "disk", "DiskStore", ItemKind::Type),
            item("app::main", "app", "main", ItemKind::Function),
        ]),
        facts: BTreeMap::from([
            fact(
                "f-impl",
                "disk",
                it("disk::DiskStore"),
                FactKind::Implements,
                it("store::Store"),
            ),
            fact(
                "f-construct",
                "app",
                it("app::main"),
                FactKind::Construct,
                it("disk::DiskStore"),
            ),
            fact(
                "f-write",
                "app",
                it("app::main"),
                FactKind::Call(Dispatch::Foreign),
                Endpoint::External("std::fs::write".into()),
            ),
        ]),
        routes: BTreeMap::from([
            (
                RouteId("r-impl".into()),
                route(it("disk::DiskStore"), vec![(it("store::Store"), "f-impl")]),
            ),
            (
                RouteId("r-wire".into()),
                route(
                    it("disk::DiskStore"),
                    vec![(it("app::main"), "f-construct")],
                ),
            ),
            (
                RouteId("r-write".into()),
                route(
                    it("app::main"),
                    vec![(Endpoint::External("std::fs::write".into()), "f-write")],
                ),
            ),
        ]),
        gaps: vec![CoverageGap {
            unit: Some(UnitId("app/lib".into())),
            site: None,
            reason: "macro not expanded".into(),
        }],
    }
}
