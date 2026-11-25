use super::*;
use project_graph::{FactKind, ItemId, UnitId};

fn place(owner: &str, name: &str) -> Endpoint {
    Endpoint::Place {
        owner: ItemId(owner.into()),
        projection: name.into(),
    }
}

fn item(name: &str) -> Endpoint {
    Endpoint::Item(ItemId(name.into()))
}

fn flow(from: Endpoint, to: Endpoint, kind: Option<FactKind>) -> Flow {
    Flow {
        fact: kind.map(|kind| Fact {
            unit: UnitId("unit".into()),
            subject: from.clone(),
            kind,
            object: to.clone(),
            site: None,
        }),
        from,
        to,
        call: None,
        joins_instances: false,
    }
}

const LIMITS: Limits = Limits {
    max_states: 1000,
    max_call_depth: 4,
};

fn traced(flows: &[Flow], receiver: Endpoint) -> Vec<Trace> {
    trace(flows, &[Use { receiver }], LIMITS).remove(0)
}

/// main builds `Parts { backups: Box::new(OsBackups) as Box<dyn Backups> }`,
/// and consume calls `parts.backups.write()`.
fn through_a_field() -> Vec<Flow> {
    vec![
        flow(
            item("OsBackups"),
            place("main", "boxed"),
            Some(FactKind::Coerce),
        ),
        Flow {
            joins_instances: true,
            ..flow(
                place("main", "boxed"),
                place("Parts", "backups"),
                Some(FactKind::Store),
            )
        },
        flow(
            place("Parts", "backups"),
            place("consume", "_4"),
            Some(FactKind::Read),
        ),
        flow(place("consume", "_4"), place("consume", "receiver"), None),
    ]
}

#[test]
fn a_value_stored_in_a_field_and_read_elsewhere_is_a_possible_path() {
    let traces = traced(&through_a_field(), place("consume", "receiver"));
    assert_eq!(
        traces,
        [Trace::Reached {
            origin: item("OsBackups"),
            path: vec![0, 1, 2, 3],
            possible: true,
        }]
    );
}

#[test]
fn a_receiver_with_no_incoming_flow_stops_with_a_reason() {
    let traces = traced(&through_a_field(), place("elsewhere", "x"));
    assert!(matches!(&traces[..], [Trace::Stopped { reason }] if reason.contains("no flow")));
}

/// `pass(x) -> x` is called from two places with different providers.
fn identity_called_twice() -> Vec<Flow> {
    let call = |from, to, link| Flow {
        call: Some(link),
        ..flow(from, to, Some(FactKind::Pass))
    };
    vec![
        flow(
            item("Local"),
            place("a", "value"),
            Some(FactKind::Construct),
        ),
        flow(
            item("Remote"),
            place("b", "value"),
            Some(FactKind::Construct),
        ),
        call(
            place("a", "value"),
            place("pass", "arg0"),
            CallLink::Argument("a:1".into()),
        ),
        call(
            place("b", "value"),
            place("pass", "arg0"),
            CallLink::Argument("b:1".into()),
        ),
        flow(place("pass", "arg0"), place("pass", "return"), None),
        call(
            place("pass", "return"),
            place("a", "result"),
            CallLink::Returned("a:1".into()),
        ),
        call(
            place("pass", "return"),
            place("b", "result"),
            CallLink::Returned("b:1".into()),
        ),
    ]
}

#[test]
fn a_returned_value_is_followed_into_its_own_call_only() {
    let traces = traced(&identity_called_twice(), place("a", "result"));
    assert_eq!(
        traces,
        [Trace::Reached {
            origin: item("Local"),
            path: vec![0, 2, 4, 5],
            possible: false,
        }]
    );
}

#[test]
fn a_parameter_with_no_known_call_reaches_every_caller() {
    let traces = traced(&identity_called_twice(), place("pass", "arg0"));
    let origins: Vec<_> = traces
        .iter()
        .map(|t| match t {
            Trace::Reached { origin, .. } => origin.clone(),
            Trace::Stopped { reason } => panic!("stopped: {reason}"),
        })
        .collect();
    assert_eq!(origins, [item("Local"), item("Remote")]);
}

#[test]
fn an_established_path_is_kept_over_a_possible_one_to_the_same_origin() {
    let mut flows = through_a_field();
    flows.push(flow(
        place("main", "boxed"),
        place("consume", "_4"),
        Some(FactKind::Pass),
    ));
    let traces = traced(&flows, place("consume", "receiver"));
    assert_eq!(
        traces,
        [Trace::Reached {
            origin: item("OsBackups"),
            path: vec![0, 4, 3],
            possible: false,
        }]
    );
}

#[test]
fn bounds_end_the_search_and_say_so() {
    let few = Limits {
        max_states: 2,
        max_call_depth: 4,
    };
    let traces = trace(
        &through_a_field(),
        &[Use {
            receiver: place("consume", "receiver"),
        }],
        few,
    );
    assert!(matches!(&traces[0][..], [Trace::Stopped { reason }] if reason.contains("2 states")));

    let shallow = Limits {
        max_states: 1000,
        max_call_depth: 0,
    };
    let traces = trace(
        &identity_called_twice(),
        &[Use {
            receiver: place("a", "result"),
        }],
        shallow,
    );
    assert!(
        matches!(&traces[0][..], [Trace::Stopped { reason }] if reason.contains("0 nested calls"))
    );
}

#[test]
fn a_cycle_of_moves_ends() {
    let mut flows = through_a_field();
    flows.push(flow(
        place("consume", "receiver"),
        place("consume", "_4"),
        None,
    ));
    let traces = traced(&flows, place("consume", "receiver"));
    assert!(matches!(&traces[..], [Trace::Reached { .. }]));
}
