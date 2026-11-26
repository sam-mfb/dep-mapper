use super::*;
use project_graph::{
    Build, Dispatch, Fact, GraphId, Hop, ItemId, ReceiverBasis, RouteId, Support, UnitId,
};
use std::collections::BTreeMap;

fn item(name: &str) -> Endpoint {
    Endpoint::Item(ItemId(name.into()))
}

fn outside(name: &str) -> Endpoint {
    Endpoint::External(name.into())
}

fn fact(subject: Endpoint, kind: FactKind, object: Endpoint) -> Fact {
    Fact {
        unit: UnitId("app/lib:app".into()),
        subject,
        kind,
        object,
        site: None,
    }
}

fn call(subject: Endpoint, object: Endpoint) -> Fact {
    fact(subject, FactKind::Call(Dispatch::Direct), object)
}

/// A route from `origin` through the hops, each `(fact, to)`.
fn route(origin: Endpoint, use_site: &str, hops: &[(&str, Endpoint)]) -> Route {
    Route {
        use_sites: vec![FactId(use_site.into())],
        origin,
        hops: hops
            .iter()
            .map(|(fact, to)| Hop {
                to: to.clone(),
                facts: vec![FactId(fact.to_string())],
                support: Support::Established,
            })
            .collect(),
        end: RouteEnd::ReachedUse,
    }
}

fn graph(facts: Vec<(&str, Fact)>, routes: Vec<(&str, Route)>) -> ProjectGraph {
    ProjectGraph {
        id: GraphId("app@1 app [default]".into()),
        build: Build {
            project: "app".into(),
            source_revision: "1".into(),
            target: "app".into(),
            configuration: "default".into(),
        },
        modules: BTreeMap::new(),
        units: BTreeMap::new(),
        items: BTreeMap::new(),
        facts: facts
            .into_iter()
            .map(|(id, f)| (FactId(id.into()), f))
            .collect(),
        routes: routes
            .into_iter()
            .map(|(id, r)| (RouteId(id.into()), r))
            .collect(),
        gaps: Vec::new(),
    }
}

fn packages(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// Rust-style names: the package is the first segment.
fn first_segment(names: &[&str]) -> impl Fn(&str) -> bool {
    let names = packages(names);
    move |name: &str| {
        names
            .iter()
            .any(|p| name.split("::").next() == Some(p.as_str()))
    }
}

/// `app::run` calls `std::mem::take` and `clock::now`; each call is a route
/// of one hop.
fn direct_uses() -> ProjectGraph {
    graph(
        vec![
            (
                "fact:std",
                call(item("app::run"), outside("std::mem::take")),
            ),
            ("fact:clock", call(item("app::run"), outside("clock::now"))),
        ],
        vec![
            (
                "route:std",
                route(
                    outside("std::mem::take"),
                    "fact:std",
                    &[("fact:std", item("app::run"))],
                ),
            ),
            (
                "route:clock",
                route(
                    outside("clock::now"),
                    "fact:clock",
                    &[("fact:clock", item("app::run"))],
                ),
            ),
        ],
    )
}

#[test]
fn a_route_from_an_excluded_package_and_its_only_fact_are_left_out() {
    let mut graph = direct_uses();
    let removed = exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert_eq!(
        removed,
        Removed {
            routes: 1,
            facts: 1
        }
    );
    assert_eq!(
        graph
            .routes
            .keys()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        ["route:clock"]
    );
    assert_eq!(
        graph.facts.keys().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        ["fact:clock"]
    );
}

#[test]
fn the_graph_records_the_packages_and_what_was_removed() {
    let mut graph = direct_uses();
    exclude(
        &mut graph,
        &packages(&["std", "serde"]),
        first_segment(&["std", "serde"]),
    );
    assert_eq!(
        graph.gaps,
        [CoverageGap {
            unit: None,
            site: None,
            reason: "Routes that start in the excluded packages std, serde are left out: \
                     1 route and 1 fact removed."
                .into(),
        }]
    );
}

#[test]
fn a_list_that_removes_nothing_is_still_recorded() {
    let mut graph = direct_uses();
    let removed = exclude(&mut graph, &packages(&["tokio"]), first_segment(&["tokio"]));
    assert_eq!(
        removed,
        Removed {
            routes: 0,
            facts: 0
        }
    );
    assert_eq!(graph.routes.len(), 2);
    assert_eq!(
        graph.gaps[0].reason,
        "Routes that start in the excluded packages tokio are left out: 0 routes and 0 facts removed."
    );
}

#[test]
fn an_empty_list_leaves_the_graph_unchanged() {
    let mut graph = direct_uses();
    let before = graph.clone();
    let removed = exclude(&mut graph, &[], |_| true);
    assert_eq!(
        removed,
        Removed {
            routes: 0,
            facts: 0
        }
    );
    assert_eq!(graph, before);
}

#[test]
fn a_fact_a_kept_route_also_cites_stays() {
    // The call into `std` is also the use site of a route from `supplier`.
    let mut graph = direct_uses();
    graph.routes.insert(
        RouteId("route:traced".into()),
        route(
            item("supplier::Backups"),
            "fact:std",
            &[(
                "fact:pass",
                Endpoint::Place {
                    owner: ItemId("app::run".into()),
                    projection: "argument 0".into(),
                },
            )],
        ),
    );
    graph.facts.insert(
        FactId("fact:pass".into()),
        fact(
            item("supplier::Backups"),
            FactKind::Pass,
            Endpoint::Place {
                owner: ItemId("app::run".into()),
                projection: "argument 0".into(),
            },
        ),
    );
    let removed = exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert_eq!(
        removed,
        Removed {
            routes: 1,
            facts: 0
        }
    );
    assert!(graph.facts.contains_key(&FactId("fact:std".into())));
}

#[test]
fn a_fact_between_project_endpoints_stays_when_its_route_starts_in_an_excluded_package() {
    // A value made by `tokio` passes from `app` to `worker` and is used there.
    let mut graph = graph(
        vec![
            (
                "fact:made",
                fact(
                    outside("tokio::sync::channel"),
                    FactKind::Return,
                    item("app::run"),
                ),
            ),
            (
                "fact:pass",
                fact(item("app::run"), FactKind::Pass, item("worker::start")),
            ),
            (
                "fact:use",
                call(item("worker::start"), outside("tokio::sync::Sender::send")),
            ),
        ],
        vec![(
            "route:tokio",
            route(
                outside("tokio::sync::channel"),
                "fact:use",
                &[
                    ("fact:made", item("app::run")),
                    ("fact:pass", item("worker::start")),
                ],
            ),
        )],
    );
    let removed = exclude(&mut graph, &packages(&["tokio"]), first_segment(&["tokio"]));
    assert_eq!(
        removed,
        Removed {
            routes: 1,
            facts: 2
        }
    );
    assert!(graph.routes.is_empty());
    assert_eq!(
        graph.facts.keys().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        ["fact:pass"]
    );
}

#[test]
fn a_fact_another_remaining_fact_cites_stays() {
    let mut graph = direct_uses();
    graph.facts.insert(
        FactId("fact:bound".into()),
        fact(
            item("app::run"),
            FactKind::ReceiverBound {
                call: FactId("fact:std".into()),
                basis: ReceiverBasis::SelectedByCompiler,
            },
            item("supplier::Backups"),
        ),
    );
    let removed = exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert_eq!(
        removed,
        Removed {
            routes: 1,
            facts: 0
        }
    );
    assert!(graph.facts.contains_key(&FactId("fact:std".into())));
}

#[test]
fn a_fact_no_route_cites_stays() {
    let mut graph = direct_uses();
    graph.facts.insert(
        FactId("fact:loose".into()),
        call(item("app::run"), outside("std::process::exit")),
    );
    exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert!(graph.facts.contains_key(&FactId("fact:loose".into())));
}

#[test]
fn a_route_stopped_at_a_fact_cites_it() {
    let mut graph = direct_uses();
    graph.routes.insert(
        RouteId("route:stopped".into()),
        Route {
            use_sites: Vec::new(),
            origin: item("supplier::Backups"),
            hops: Vec::new(),
            end: RouteEnd::Stopped {
                at: vec![FactId("fact:std".into())],
                reason: "no origin".into(),
            },
        },
    );
    let removed = exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert_eq!(
        removed,
        Removed {
            routes: 1,
            facts: 0
        }
    );
}

#[test]
fn only_outside_origins_are_matched() {
    // A project item whose name happens to start like an excluded package.
    let mut graph = graph(
        vec![("fact:a", call(item("std::shim"), item("app::run")))],
        vec![(
            "route:a",
            route(item("std::shim"), "fact:a", &[("fact:a", item("app::run"))]),
        )],
    );
    let removed = exclude(&mut graph, &packages(&["std"]), first_segment(&["std"]));
    assert_eq!(
        removed,
        Removed {
            routes: 0,
            facts: 0
        }
    );
}
