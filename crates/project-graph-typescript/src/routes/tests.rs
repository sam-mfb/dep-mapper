use super::*;
use project_graph::{Dispatch, UnitId};

fn place(owner: &str, projection: &str) -> Endpoint {
    Endpoint::Place {
        owner: ItemId(owner.into()),
        projection: projection.into(),
    }
}

fn item(id: &str) -> Endpoint {
    Endpoint::Item(ItemId(id.into()))
}

fn fact(subject: Endpoint, kind: FactKind, object: Endpoint) -> Fact {
    Fact {
        unit: UnitId("unit".into()),
        subject,
        kind,
        object,
        site: None,
    }
}

fn facts(list: Vec<(&str, Fact)>) -> BTreeMap<FactId, Fact> {
    list.into_iter()
        .map(|(id, fact)| (FactId(id.into()), fact))
        .collect()
}

fn receiver(call: &str, moves: &[&str]) -> ReceiverUse {
    ReceiverUse {
        call: FactId(call.into()),
        moves: moves.iter().map(|m| FactId((*m).into())).collect(),
    }
}

fn hop_facts(route: &Route) -> Vec<&str> {
    route.hops.iter().map(|h| h.fact.0.as_str()).collect()
}

const LIMITS: TraceLimits = TraceLimits::STANDARD;

#[test]
fn a_value_route_runs_forward_from_construction_to_the_receiver() {
    let graph = facts(vec![
        (
            "new",
            fact(item("Os"), FactKind::Construct, place("main", "os")),
        ),
        (
            "arg",
            fact(place("main", "os"), FactKind::Pass, place("Use.new", "b")),
        ),
        (
            "field",
            fact(place("Use.new", "b"), FactKind::Store, place("Use", "b")),
        ),
        (
            "recv",
            fact(
                place("Use", "b"),
                FactKind::Pass,
                place("Api.write", "self"),
            ),
        ),
        (
            "call",
            fact(
                item("Use.run"),
                FactKind::Call(Dispatch::Contract),
                item("Api.write"),
            ),
        ),
    ]);
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        LIMITS,
    );
    assert!(traced.gaps.is_empty());
    let route = &traced.routes[&RouteId("value:call:0".into())];
    assert_eq!(route.origin, item("Os"));
    assert_eq!(route.use_site, Some(FactId("call".into())));
    assert_eq!(hop_facts(route), ["new", "arg", "field", "recv"]);
    assert_eq!(route.hops.last().unwrap().to, place("Api.write", "self"));
    assert_eq!(route.end, RouteEnd::ReachedUse);
    assert!(route.hops.iter().all(|h| h.support == Support::Established));
}

#[test]
fn each_branch_of_a_conditional_is_its_own_possible_route() {
    let graph = facts(vec![
        (
            "mem",
            fact(item("Mem"), FactKind::Construct, place("main", "b")),
        ),
        (
            "os",
            fact(item("Os"), FactKind::Construct, place("main", "os")),
        ),
        (
            "coerce",
            fact(place("main", "os"), FactKind::Coerce, place("main", "b")),
        ),
        (
            "recv",
            fact(
                place("main", "b"),
                FactKind::Pass,
                place("Api.write", "self"),
            ),
        ),
    ]);
    let conditions = BTreeMap::from([
        (FactId("mem".into()), "dry".to_string()),
        (FactId("coerce".into()), "!dry".to_string()),
    ]);
    let traced = trace_values(&graph, &conditions, &[receiver("call", &["recv"])], LIMITS);
    let origins: Vec<_> = traced.routes.values().map(|r| r.origin.clone()).collect();
    assert_eq!(origins.len(), 2);
    assert!(origins.contains(&item("Mem")) && origins.contains(&item("Os")));
    let from_os = traced
        .routes
        .values()
        .find(|r| r.origin == item("Os"))
        .unwrap();
    assert_eq!(hop_facts(from_os), ["os", "coerce", "recv"]);
    assert_eq!(
        from_os.hops[1].support,
        Support::Possible {
            condition: Some("!dry".into())
        }
    );
}

#[test]
fn a_trace_with_no_incoming_value_stops_where_it_ran_out() {
    let graph = facts(vec![(
        "recv",
        fact(
            place("helper", "b"),
            FactKind::Pass,
            place("Api.write", "self"),
        ),
    )]);
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        LIMITS,
    );
    let route = traced.routes.values().next().unwrap();
    assert_eq!(route.origin, place("helper", "b"));
    assert_eq!(
        route.end,
        RouteEnd::Stopped {
            at: FactId("recv".into()),
            reason: "no recorded value enters b in helper".into()
        }
    );
}

#[test]
fn a_cycle_of_moves_ends_the_trace_instead_of_repeating() {
    let graph = facts(vec![
        (
            "ab",
            fact(place("f", "a"), FactKind::Store, place("f", "b")),
        ),
        (
            "ba",
            fact(place("f", "b"), FactKind::Store, place("f", "a")),
        ),
        (
            "recv",
            fact(place("f", "a"), FactKind::Pass, place("Api.write", "self")),
        ),
    ]);
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        LIMITS,
    );
    assert_eq!(traced.routes.len(), 1);
    let route = traced.routes.values().next().unwrap();
    assert_eq!(hop_facts(route), ["ab", "ba", "recv"]);
    assert!(matches!(route.end, RouteEnd::Stopped { .. }));
}

#[test]
fn a_value_from_outside_the_project_is_an_origin() {
    let graph = facts(vec![(
        "recv",
        fact(
            Endpoint::External("clock#systemClock".into()),
            FactKind::Pass,
            place("Api.write", "self"),
        ),
    )]);
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        LIMITS,
    );
    let route = traced.routes.values().next().unwrap();
    assert_eq!(route.origin, Endpoint::External("clock#systemClock".into()));
    assert_eq!(route.end, RouteEnd::ReachedUse);
}

#[test]
fn routes_beyond_the_limit_are_reported_as_a_gap() {
    let mut list = vec![(
        "recv".to_string(),
        fact(place("f", "b"), FactKind::Pass, place("Api.write", "self")),
    )];
    for n in 0..3 {
        list.push((
            format!("new{n}"),
            fact(item(&format!("C{n}")), FactKind::Construct, place("f", "b")),
        ));
    }
    let graph: BTreeMap<_, _> = list.into_iter().map(|(id, f)| (FactId(id), f)).collect();
    let limits = TraceLimits {
        max_hops: 32,
        max_routes_per_use: 2,
    };
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        limits,
    );
    assert_eq!(traced.routes.len(), 2);
    assert_eq!(traced.gaps.len(), 1);
    assert!(traced.gaps[0].reason.starts_with("3 value routes"));
}

#[test]
fn a_long_chain_stops_at_the_hop_limit() {
    let graph = facts(vec![
        (
            "ab",
            fact(place("f", "a"), FactKind::Store, place("f", "b")),
        ),
        (
            "bc",
            fact(place("f", "b"), FactKind::Store, place("f", "c")),
        ),
        (
            "recv",
            fact(place("f", "c"), FactKind::Pass, place("Api.write", "self")),
        ),
    ]);
    let limits = TraceLimits {
        max_hops: 2,
        max_routes_per_use: 16,
    };
    let traced = trace_values(
        &graph,
        &BTreeMap::new(),
        &[receiver("call", &["recv"])],
        limits,
    );
    let route = traced.routes.values().next().unwrap();
    assert_eq!(hop_facts(route), ["bc", "recv"]);
    assert!(
        matches!(&route.end, RouteEnd::Stopped { reason, .. } if reason == "the search reached its hop limit")
    );
}

fn module_of(endpoint: &Endpoint) -> Option<ModuleId> {
    let id = match endpoint {
        Endpoint::Module(m) => return Some(m.clone()),
        Endpoint::Item(i) => &i.0,
        Endpoint::Place { owner, .. } => &owner.0,
        _ => return None,
    };
    Some(ModuleId(id.split('/').next().unwrap().into()))
}

#[test]
fn a_direct_use_across_modules_is_a_route_of_one_hop() {
    let graph = facts(vec![
        (
            "cross",
            fact(
                item("app/main"),
                FactKind::Call(Dispatch::Direct),
                item("os/OsBackups"),
            ),
        ),
        (
            "inside",
            fact(
                item("app/main"),
                FactKind::Call(Dispatch::Direct),
                item("app/helper"),
            ),
        ),
        (
            "outside",
            fact(
                item("app/main"),
                FactKind::Call(Dispatch::Foreign),
                Endpoint::External("clock#now".into()),
            ),
        ),
        (
            "move",
            fact(
                item("os/OsBackups"),
                FactKind::Construct,
                place("app/main", "os"),
            ),
        ),
    ]);
    let routes = direct_routes(&graph, module_of);
    let ids: Vec<_> = routes.keys().map(|r| r.0.as_str()).collect();
    assert_eq!(ids, ["use:cross", "use:outside"]);
    let route = &routes[&RouteId("use:cross".into())];
    assert_eq!(route.origin, item("os/OsBackups"));
    assert_eq!(route.hops.len(), 1);
    assert_eq!(route.hops[0].to, item("app/main"));
    assert_eq!(route.use_site, Some(FactId("cross".into())));
}

#[test]
fn one_implementer_is_unique_and_several_are_candidates() {
    let declared_by = |method: &ItemId| method.0.split_once('.').map(|(t, _)| ItemId(t.into()));
    let contract_call = |method: &str| {
        fact(
            item("Use.run"),
            FactKind::Call(Dispatch::Contract),
            item(method),
        )
    };
    let graph = facts(vec![
        ("c1", fact(item("Os"), FactKind::Implements, item("Api"))),
        ("c2", fact(item("Mem"), FactKind::Implements, item("Api"))),
        ("c3", fact(item("Os"), FactKind::Implements, item("Log"))),
        ("write", contract_call("Api.write")),
        ("log", contract_call("Log.line")),
        (
            "direct",
            fact(
                item("Use.run"),
                FactKind::Call(Dispatch::Direct),
                item("Api.write"),
            ),
        ),
    ]);
    let bounds = receiver_bounds(&graph, declared_by);
    let basis = |call: &str, class: &str| match &bounds
        [&FactId(format!("receiver_bound:{call}:{class}"))]
        .kind
    {
        FactKind::ReceiverBound { basis, .. } => basis.clone(),
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(bounds.len(), 3);
    assert_eq!(basis("write", "Os"), ReceiverBasis::BuildCandidate);
    assert_eq!(basis("write", "Mem"), ReceiverBasis::BuildCandidate);
    assert_eq!(basis("log", "Os"), ReceiverBasis::UniqueInClosedBuild);
}
