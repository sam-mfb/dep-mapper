use super::*;
use project_graph::{Hop, ItemKind, Visibility};
use std::collections::BTreeSet;

/// Merges routes whose endpoints name no known items, so none of them lies
/// in a project module and only merging by path applies.
fn merge(routes: BTreeMap<RouteId, Route>) -> BTreeMap<RouteId, Route> {
    super::merge(routes, &BTreeMap::new())
}

fn item(name: &str) -> Endpoint {
    Endpoint::Item(ItemId(name.into()))
}

fn ids(names: &[&str]) -> Vec<FactId> {
    names.iter().map(|n| FactId(n.to_string())).collect()
}

fn hop(to: &str, fact: &str) -> Hop {
    Hop {
        to: item(to),
        facts: ids(&[fact]),
        support: Support::Established,
    }
}

/// A route from `origin` used at `site`, through `hops`, reaching its use.
fn reached(origin: &str, site: &str, hops: Vec<Hop>) -> Route {
    Route {
        use_sites: ids(&[site]),
        origin: item(origin),
        hops,
        end: RouteEnd::ReachedUse,
    }
}

fn stopped(origin: &str, site: &str, reason: &str) -> Route {
    Route {
        use_sites: ids(&[site]),
        origin: item(origin),
        hops: Vec::new(),
        end: RouteEnd::Stopped {
            at: ids(&[site]),
            reason: reason.into(),
        },
    }
}

fn routes(list: Vec<(&str, Route)>) -> BTreeMap<RouteId, Route> {
    list.into_iter()
        .map(|(id, r)| (RouteId(id.into()), r))
        .collect()
}

/// Every fact a set of routes cites.
fn cited(routes: &BTreeMap<RouteId, Route>) -> BTreeSet<FactId> {
    routes
        .values()
        .flat_map(|r| {
            let at = match &r.end {
                RouteEnd::Stopped { at, .. } => at.clone(),
                RouteEnd::ReachedUse => Vec::new(),
            };
            r.use_sites
                .iter()
                .cloned()
                .chain(r.hops.iter().flat_map(|h| h.facts.clone()))
                .chain(at)
        })
        .collect()
}

#[test]
fn routes_on_the_same_path_merge_and_keep_every_site_and_fact() {
    let merged = merge(routes(vec![
        (
            "route:1",
            reached("a", "use:1", vec![hop("b", "f1"), hop("c", "f2")]),
        ),
        (
            "route:2",
            reached("a", "use:2", vec![hop("b", "f3"), hop("c", "f4")]),
        ),
    ]));
    assert_eq!(
        merged,
        routes(vec![(
            "route:1",
            Route {
                use_sites: ids(&["use:1", "use:2"]),
                origin: item("a"),
                hops: vec![
                    Hop {
                        to: item("b"),
                        facts: ids(&["f1", "f3"]),
                        support: Support::Established,
                    },
                    Hop {
                        to: item("c"),
                        facts: ids(&["f2", "f4"]),
                        support: Support::Established,
                    },
                ],
                end: RouteEnd::ReachedUse,
            }
        )])
    );
}

#[test]
fn a_fact_shared_by_merged_routes_is_cited_once() {
    let merged = merge(routes(vec![
        ("route:1", reached("a", "use:1", vec![hop("b", "f1")])),
        ("route:2", reached("a", "use:1", vec![hop("b", "f1")])),
        ("route:3", reached("a", "use:2", vec![hop("b", "f2")])),
    ]));
    let route = &merged[&RouteId("route:1".into())];
    assert_eq!(merged.len(), 1);
    assert_eq!(route.use_sites, ids(&["use:1", "use:2"]));
    assert_eq!(route.hops[0].facts, ids(&["f1", "f2"]));
}

#[test]
fn routes_with_no_use_site_merge_too() {
    let mut first = reached("a", "x", vec![hop("b", "f1")]);
    let mut second = reached("a", "x", vec![hop("b", "f2")]);
    first.use_sites.clear();
    second.use_sites.clear();
    let merged = merge(routes(vec![("route:1", first), ("route:2", second)]));
    assert_eq!(merged.len(), 1);
    let route = &merged[&RouteId("route:1".into())];
    assert!(route.use_sites.is_empty());
    assert_eq!(route.hops[0].facts, ids(&["f1", "f2"]));
}

#[test]
fn routes_that_differ_in_any_step_stay_separate() {
    let possible = Hop {
        support: Support::Possible { condition: None },
        ..hop("b", "f6")
    };
    let conditional = Hop {
        support: Support::Possible {
            condition: Some("when built with tls".into()),
        },
        ..hop("b", "f7")
    };
    let before = routes(vec![
        ("route:1", reached("a", "use:1", vec![hop("b", "f1")])),
        // Another origin.
        ("route:2", reached("z", "use:2", vec![hop("b", "f2")])),
        // Another endpoint.
        ("route:3", reached("a", "use:3", vec![hop("c", "f3")])),
        // A longer path through the same first step.
        (
            "route:4",
            reached("a", "use:4", vec![hop("b", "f4"), hop("c", "f5")]),
        ),
        // Other support.
        ("route:5", reached("a", "use:5", vec![possible])),
        ("route:6", reached("a", "use:6", vec![conditional])),
        // Another end.
        (
            "route:7",
            Route {
                end: RouteEnd::Stopped {
                    at: ids(&["use:7"]),
                    reason: "no origin".into(),
                },
                ..reached("a", "use:7", vec![hop("b", "f8")])
            },
        ),
    ]);
    assert_eq!(merge(before.clone()), before);
}

#[test]
fn stopped_routes_merge_only_when_they_stop_for_the_same_reason() {
    let merged = merge(routes(vec![
        ("route:1", stopped("a", "use:1", "no origin")),
        ("route:2", stopped("a", "use:2", "no origin")),
        ("route:3", stopped("a", "use:3", "search bound reached")),
    ]));
    assert_eq!(
        merged,
        routes(vec![
            (
                "route:1",
                Route {
                    use_sites: ids(&["use:1", "use:2"]),
                    origin: item("a"),
                    hops: Vec::new(),
                    end: RouteEnd::Stopped {
                        at: ids(&["use:1", "use:2"]),
                        reason: "no origin".into(),
                    },
                }
            ),
            ("route:3", stopped("a", "use:3", "search bound reached")),
        ])
    );
}

#[test]
fn merging_keeps_every_cited_fact_and_the_first_id_of_each_group() {
    let before = routes(vec![
        ("route:b", reached("a", "use:1", vec![hop("b", "f1")])),
        ("route:a", reached("a", "use:2", vec![hop("b", "f2")])),
        ("route:c", reached("a", "use:3", vec![hop("c", "f3")])),
        ("route:d", stopped("a", "use:4", "no origin")),
        ("route:e", stopped("a", "use:5", "no origin")),
    ]);
    let merged = merge(before.clone());
    assert_eq!(cited(&merged), cited(&before));
    assert_eq!(
        merged.keys().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        ["route:a", "route:c", "route:d"]
    );
}

/// Items `app::main` and `app::run` in module `app`, and `store::Save` in
/// module `store`.
fn items() -> BTreeMap<ItemId, Item> {
    [
        ("app::main", "app"),
        ("app::run", "app"),
        ("store::Save", "store"),
    ]
    .into_iter()
    .map(|(id, module)| {
        let item = Item {
            module: ModuleId(module.into()),
            name: id.into(),
            kind: ItemKind::Function,
            visibility: Visibility::Exported,
            signature: None,
            declaration: None,
        };
        (ItemId(id.into()), item)
    })
    .collect()
}

fn outside(name: &str) -> Endpoint {
    Endpoint::External(name.into())
}

fn module(name: &str) -> Endpoint {
    Endpoint::Module(ModuleId(name.into()))
}

/// A route from `origin` used at `site`, through `hops`, each `(to, fact)`.
fn route(origin: Endpoint, site: &str, hops: &[(&str, &str)]) -> Route {
    Route {
        origin,
        ..reached(
            "unused",
            site,
            hops.iter().map(|(to, f)| hop(to, f)).collect(),
        )
    }
}

#[test]
fn an_outside_value_used_only_inside_one_module_ends_at_the_module() {
    let before = routes(vec![
        (
            "route:1",
            route(outside("clap::Arg"), "use:1", &[("app::main", "f1")]),
        ),
        (
            "route:2",
            route(outside("clap::Arg"), "use:2", &[("app::run", "f2")]),
        ),
        (
            "route:3",
            route(
                outside("clap::Arg"),
                "use:3",
                &[("app::main", "f3"), ("app::run", "f4")],
            ),
        ),
    ]);
    let merged = super::merge(before.clone(), &items());
    assert_eq!(
        merged,
        routes(vec![
            (
                "route:1",
                Route {
                    use_sites: ids(&["use:1", "use:2"]),
                    origin: outside("clap::Arg"),
                    hops: vec![Hop {
                        to: module("app"),
                        facts: ids(&["f1", "f2"]),
                        support: Support::Established,
                    }],
                    end: RouteEnd::ReachedUse,
                }
            ),
            // A longer path in the module ends at the module too, step by step.
            (
                "route:3",
                Route {
                    use_sites: ids(&["use:3"]),
                    origin: outside("clap::Arg"),
                    hops: vec![
                        Hop {
                            to: module("app"),
                            facts: ids(&["f3"]),
                            support: Support::Established,
                        },
                        Hop {
                            to: module("app"),
                            facts: ids(&["f4"]),
                            support: Support::Established,
                        },
                    ],
                    end: RouteEnd::ReachedUse,
                }
            ),
        ])
    );
    assert_eq!(cited(&merged), cited(&before));
}

#[test]
fn a_route_from_inside_its_module_ends_at_the_module() {
    let merged = super::merge(
        routes(vec![(
            "route:1",
            route(item("app::run"), "use:1", &[("app::main", "f1")]),
        )]),
        &items(),
    );
    assert_eq!(merged[&RouteId("route:1".into())].hops[0].to, module("app"));
}

#[test]
fn a_value_that_crosses_modules_keeps_its_detail() {
    let before = routes(vec![
        // From an outside crate through `app` into `store`.
        (
            "route:1",
            route(
                outside("clap::Arg"),
                "use:1",
                &[("app::main", "f1"), ("store::Save", "f2")],
            ),
        ),
        // From `store` into `app`.
        (
            "route:2",
            route(item("store::Save"), "use:2", &[("app::main", "f3")]),
        ),
        (
            "route:3",
            route(item("store::Save"), "use:3", &[("app::run", "f4")]),
        ),
        // Through `app` and out of the project.
        (
            "route:4",
            route(
                outside("clap::Arg"),
                "use:4",
                &[("app::main", "f5"), ("unknown::thing", "f6")],
            ),
        ),
    ]);
    assert_eq!(super::merge(before.clone(), &items()), before);
}

#[test]
fn a_stopped_route_keeps_its_detail() {
    let mut stopped = route(outside("clap::Arg"), "use:1", &[("app::main", "f1")]);
    stopped.end = RouteEnd::Stopped {
        at: ids(&["use:1"]),
        reason: "no origin".into(),
    };
    let before = routes(vec![("route:1", stopped)]);
    assert_eq!(super::merge(before.clone(), &items()), before);
}
