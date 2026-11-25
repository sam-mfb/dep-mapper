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

fn argument(site: &str) -> MoveLink {
    MoveLink {
        argument: Some(site.into()),
        returned: None,
    }
}

fn returned(site: &str) -> MoveLink {
    MoveLink {
        argument: None,
        returned: Some(site.into()),
    }
}

/// Fields are places owned by an item whose name starts with a capital.
fn is_field(endpoint: &Endpoint) -> bool {
    matches!(endpoint, Endpoint::Place { owner, .. } if owner.0.starts_with(char::is_uppercase))
}

fn trace(
    facts: &BTreeMap<FactId, Fact>,
    links: &[(&str, MoveLink)],
    conditions: &[(&str, &str)],
    uses: &[ReceiverUse],
) -> BTreeMap<RouteId, Route> {
    let links = links
        .iter()
        .map(|(id, link)| (FactId((*id).into()), link.clone()))
        .collect();
    let conditions = conditions
        .iter()
        .map(|(id, c)| (FactId((*id).into()), c.to_string()))
        .collect();
    let moves = ValueMoves {
        facts,
        links: &links,
        conditions: &conditions,
    };
    trace_values(&moves, uses, is_field, TraceLimits::STANDARD)
}

#[test]
fn a_value_route_runs_forward_from_construction_to_the_receiver() {
    let graph = facts(vec![
        (
            "new",
            fact(item("Os"), FactKind::Construct, place("main", "os")),
        ),
        (
            "arg",
            fact(place("main", "os"), FactKind::Pass, place("use_new", "b")),
        ),
        (
            "field",
            fact(place("use_new", "b"), FactKind::Store, place("Use", "b")),
        ),
        (
            "recv",
            fact(
                place("Use", "b"),
                FactKind::Pass,
                place("api_write", "self"),
            ),
        ),
    ]);
    let routes = trace(&graph, &[], &[], &[receiver("call", &["recv"])]);
    let route = &routes[&RouteId("value:call:0".into())];
    assert_eq!(route.origin, item("Os"));
    assert_eq!(route.use_site, Some(FactId("call".into())));
    assert_eq!(hop_facts(route), ["new", "arg", "field", "recv"]);
    assert_eq!(route.hops.last().unwrap().to, place("api_write", "self"));
    assert_eq!(route.end, RouteEnd::ReachedUse);
    let support: Vec<_> = route.hops.iter().map(|h| h.support.clone()).collect();
    let possible = Support::Possible { condition: None };
    assert_eq!(
        support,
        [
            Support::Established,
            Support::Established,
            possible.clone(),
            possible
        ],
        "moves into and out of a field are possible"
    );
}

#[test]
fn a_conditional_move_is_possible_under_its_condition() {
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
                place("api_write", "self"),
            ),
        ),
    ]);
    let routes = trace(
        &graph,
        &[],
        &[("mem", "dry"), ("coerce", "!dry")],
        &[receiver("call", &["recv"])],
    );
    assert_eq!(routes.len(), 2);
    let from_os = routes.values().find(|r| r.origin == item("Os")).unwrap();
    assert_eq!(hop_facts(from_os), ["os", "coerce", "recv"]);
    assert_eq!(
        from_os.hops[1].support,
        Support::Possible {
            condition: Some("!dry".into())
        }
    );
}

#[test]
fn a_returned_value_is_matched_to_its_own_call() {
    // `const a = id(new A()); const b = id(new B()); b.go()` reaches only B.
    let graph = facts(vec![
        (
            "newA",
            fact(item("A"), FactKind::Construct, place("id", "x")),
        ),
        (
            "newB",
            fact(item("B"), FactKind::Construct, place("id", "x")),
        ),
        (
            "ret",
            fact(place("id", "x"), FactKind::Return, place("id", "return")),
        ),
        (
            "outA",
            fact(place("id", "return"), FactKind::Return, place("main", "a")),
        ),
        (
            "outB",
            fact(place("id", "return"), FactKind::Return, place("main", "b")),
        ),
        (
            "recv",
            fact(place("main", "b"), FactKind::Pass, place("go", "self")),
        ),
    ]);
    let links = [
        ("newA", argument("call-a")),
        ("newB", argument("call-b")),
        ("outA", returned("call-a")),
        ("outB", returned("call-b")),
    ];
    let routes = trace(&graph, &links, &[], &[receiver("call", &["recv"])]);
    let origins: Vec<_> = routes.values().map(|r| r.origin.clone()).collect();
    assert_eq!(origins, [item("B")]);
}

#[test]
fn a_result_passed_straight_into_another_call_is_matched_at_both_calls() {
    // `const a = echo(echo(new A())); const b = echo(echo(new B())); b.go()`
    // Calls: a1 = inner for A, a2 = outer for A, b1 and b2 likewise.
    let both = |returned: &str, argument: &str| MoveLink {
        argument: Some(argument.into()),
        returned: Some(returned.into()),
    };
    let graph = facts(vec![
        (
            "newA",
            fact(item("A"), FactKind::Construct, place("echo", "x")),
        ),
        (
            "newB",
            fact(item("B"), FactKind::Construct, place("echo", "x")),
        ),
        (
            "ret",
            fact(
                place("echo", "x"),
                FactKind::Return,
                place("echo", "return"),
            ),
        ),
        (
            "innerA",
            fact(
                place("echo", "return"),
                FactKind::Return,
                place("echo", "x"),
            ),
        ),
        (
            "innerB",
            fact(
                place("echo", "return"),
                FactKind::Return,
                place("echo", "x"),
            ),
        ),
        (
            "outA",
            fact(
                place("echo", "return"),
                FactKind::Return,
                place("main", "a"),
            ),
        ),
        (
            "outB",
            fact(
                place("echo", "return"),
                FactKind::Return,
                place("main", "b"),
            ),
        ),
        (
            "recv",
            fact(place("main", "b"), FactKind::Pass, place("go", "self")),
        ),
    ]);
    let links = [
        ("newA", argument("a1")),
        ("newB", argument("b1")),
        ("innerA", both("a1", "a2")),
        ("innerB", both("b1", "b2")),
        ("outA", returned("a2")),
        ("outB", returned("b2")),
    ];
    let routes = trace(&graph, &links, &[], &[receiver("call", &["recv"])]);
    let origins: Vec<_> = routes.values().map(|r| r.origin.clone()).collect();
    assert_eq!(origins, [item("B")]);
    let route = routes.values().next().unwrap();
    // The split through the inner call's result adds no hop of its own.
    assert_eq!(
        hop_facts(route),
        ["newB", "ret", "innerB", "ret", "outB", "recv"]
    );
}

#[test]
fn a_call_result_used_as_a_receiver_is_matched_to_its_call() {
    // `echo(new A()).go(); echo(new B()).go()`
    let graph = facts(vec![
        (
            "newA",
            fact(item("A"), FactKind::Construct, place("echo", "x")),
        ),
        (
            "newB",
            fact(item("B"), FactKind::Construct, place("echo", "x")),
        ),
        (
            "ret",
            fact(
                place("echo", "x"),
                FactKind::Return,
                place("echo", "return"),
            ),
        ),
        (
            "recvA",
            fact(place("echo", "return"), FactKind::Pass, place("go", "self")),
        ),
        (
            "recvB",
            fact(place("echo", "return"), FactKind::Pass, place("go", "self")),
        ),
    ]);
    let both = |returned: &str, argument: &str| MoveLink {
        argument: Some(argument.into()),
        returned: Some(returned.into()),
    };
    let links = [
        ("newA", argument("echoA")),
        ("newB", argument("echoB")),
        ("recvA", both("echoA", "goA")),
        ("recvB", both("echoB", "goB")),
    ];
    let routes = trace(
        &graph,
        &links,
        &[],
        &[receiver("goA", &["recvA"]), receiver("goB", &["recvB"])],
    );
    assert_eq!(routes.len(), 2, "{routes:#?}");
    assert_eq!(routes[&RouteId("value:goA:0".into())].origin, item("A"));
    assert_eq!(routes[&RouteId("value:goB:0".into())].origin, item("B"));
}

#[test]
fn each_call_explains_only_its_own_receiver() {
    // Two calls to the same method: each route starts from its own receiver.
    let graph = facts(vec![
        (
            "newA",
            fact(item("A"), FactKind::Construct, place("main", "a")),
        ),
        (
            "newB",
            fact(item("B"), FactKind::Construct, place("main", "b")),
        ),
        (
            "recvA",
            fact(place("main", "a"), FactKind::Pass, place("go", "self")),
        ),
        (
            "recvB",
            fact(place("main", "b"), FactKind::Pass, place("go", "self")),
        ),
    ]);
    let routes = trace(
        &graph,
        &[],
        &[],
        &[receiver("callA", &["recvA"]), receiver("callB", &["recvB"])],
    );
    assert_eq!(routes[&RouteId("value:callA:0".into())].origin, item("A"));
    assert_eq!(routes[&RouteId("value:callB:0".into())].origin, item("B"));
    assert_eq!(routes.len(), 2);
}

#[test]
fn a_trace_that_reaches_no_origin_stops_at_the_call() {
    let graph = facts(vec![(
        "recv",
        fact(
            place("helper", "b"),
            FactKind::Pass,
            place("api_write", "self"),
        ),
    )]);
    let routes = trace(&graph, &[], &[], &[receiver("call", &["recv"])]);
    let route = routes.values().next().unwrap();
    assert_eq!(route.origin, place("helper", "b"));
    assert!(route.hops.is_empty());
    assert!(matches!(
        &route.end,
        RouteEnd::Stopped { at, .. } if at == &FactId("call".into())
    ));
}

#[test]
fn a_receiver_constructed_in_place_is_its_own_origin() {
    // `new Os().prune()`
    let graph = facts(vec![(
        "new",
        fact(item("Os"), FactKind::Construct, place("prune", "self")),
    )]);
    let routes = trace(&graph, &[], &[], &[receiver("call", &["new"])]);
    let route = routes.values().next().unwrap();
    assert_eq!(route.origin, item("Os"));
    assert_eq!(hop_facts(route), ["new"]);
    assert_eq!(route.hops[0].to, place("prune", "self"));
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
