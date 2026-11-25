//! Routes over a graph's facts. Nothing here depends on TypeScript: the input
//! is `project_graph` facts.
//!
//! - A direct use between two modules, such as an import or a call, is a
//!   route of one hop from the supplier to the user.
//! - A value route explains a method call's receiver. The shared
//!   `value-routes` tracer searches back from the receiver over the value
//!   moves to each origin it reaches, matching a value returned from a call
//!   to that call's own arguments. A trace that reaches no origin is a
//!   route that stops at the call.
//! - A receiver bound names each class that implements the contract a call
//!   is made through.

use project_graph::{
    Endpoint, Fact, FactId, FactKind, Hop, ItemId, ModuleId, ReceiverBasis, Route, RouteEnd,
    RouteId, Support,
};
use std::collections::{BTreeMap, BTreeSet};
use value_routes::{CallLink, Flow, Trace, Use};

/// A method call and the value moves that deliver its receiver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverUse {
    pub call: FactId,
    pub moves: Vec<FactId>,
}

/// The calls a value move crosses, each named by its call site. A move can
/// cross two: in `f(g())` the value leaves `g` and enters `f`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MoveLink {
    /// The call the value enters as an argument or receiver.
    pub argument: Option<String>,
    /// The call whose result the value is.
    pub returned: Option<String>,
}

/// Bounds on the search for each receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceLimits {
    /// The most search states explored for one receiver.
    pub max_states: usize,
    /// The deepest nesting of calls a path may return through.
    pub max_call_depth: usize,
}

impl TraceLimits {
    /// The limits project-graph-rust uses, which cover its real workspaces.
    pub const STANDARD: TraceLimits = TraceLimits {
        max_states: 20_000,
        max_call_depth: 8,
    };
}

/// The value moves of a graph, with what the facts themselves do not say.
pub struct ValueMoves<'a> {
    pub facts: &'a BTreeMap<FactId, Fact>,
    /// The call each linked move crosses.
    pub links: &'a BTreeMap<FactId, MoveLink>,
    /// The condition of each move that happens on only one branch.
    pub conditions: &'a BTreeMap<FactId, String>,
}

/// Value routes for each receiver use. `joins_instances` says which
/// endpoints stand for every instance of a place, such as a field of a type
/// or a variable a closure captured; a hop into or out of one is possible,
/// since the value read may belong to another instance. A hop whose move is
/// conditional is possible under that condition.
pub fn trace_values(
    moves: &ValueMoves,
    uses: &[ReceiverUse],
    joins_instances: impl Fn(&Endpoint) -> bool,
    limits: TraceLimits,
) -> BTreeMap<RouteId, Route> {
    let mut flows = Vec::new();
    let mut ids = Vec::new();
    // Adds the flows for one move. A move that leaves one call and enters
    // another goes through the first call's result, so that each crossing
    // is matched to its own call: first the return, without a fact, then the
    // argument, with the move's fact as its hop.
    let mut add = |id: &FactId, fact: &Fact, to: Endpoint, link: &MoveLink| {
        let joins = joins_instances(&fact.subject) || joins_instances(&to);
        let mut from = fact.subject.clone();
        let mut call = link.returned.clone().map(CallLink::Returned);
        if let (Some(returned), Some(argument)) = (&link.returned, &link.argument) {
            let result = Endpoint::Unresolved(format!("result of {returned}"));
            flows.push(Flow {
                from,
                to: result.clone(),
                fact: None,
                call: Some(CallLink::Returned(returned.clone())),
                joins_instances: false,
            });
            ids.push(id.clone());
            from = result;
            call = Some(CallLink::Argument(argument.clone()));
        } else if let Some(argument) = &link.argument {
            call = Some(CallLink::Argument(argument.clone()));
        }
        flows.push(Flow {
            from,
            to,
            fact: Some(fact.clone()),
            call,
            joins_instances: joins,
        });
        ids.push(id.clone());
    };
    let unlinked = MoveLink::default();
    for (id, fact) in moves.facts {
        if is_move(&fact.kind) {
            let link = moves.links.get(id).unwrap_or(&unlinked);
            add(id, fact, fact.object.clone(), link);
        }
    }
    // Each call gets a receiver of its own, so a trace explains this call
    // and not every call to the same method. The move into it keeps the
    // call it returns from, if any; the call it enters is this one.
    let receivers: Vec<Endpoint> = uses
        .iter()
        .map(|u| Endpoint::Unresolved(format!("receiver of {}", u.call.0)))
        .collect();
    for (receiver_use, receiver) in uses.iter().zip(&receivers) {
        for id in &receiver_use.moves {
            if let Some(fact) = moves.facts.get(id) {
                let link = MoveLink {
                    argument: None,
                    returned: moves.links.get(id).and_then(|l| l.returned.clone()),
                };
                add(id, fact, receiver.clone(), &link);
            }
        }
    }
    let traces = value_routes::trace(
        &flows,
        &receivers
            .iter()
            .map(|receiver| Use {
                receiver: receiver.clone(),
            })
            .collect::<Vec<_>>(),
        value_routes::Limits {
            max_states: limits.max_states,
            max_call_depth: limits.max_call_depth,
        },
    );

    let mut routes = BTreeMap::new();
    for (receiver_use, traces) in uses.iter().zip(traces) {
        for (index, trace) in traces.into_iter().enumerate() {
            let route = match trace {
                Trace::Reached { origin, path, .. } => Route {
                    use_site: Some(receiver_use.call.clone()),
                    origin,
                    hops: path
                        .iter()
                        .filter_map(|&i| {
                            let fact = flows[i].fact.as_ref()?;
                            let support = match moves.conditions.get(&ids[i]) {
                                Some(condition) => Support::Possible {
                                    condition: Some(condition.clone()),
                                },
                                None if flows[i].joins_instances => {
                                    Support::Possible { condition: None }
                                }
                                None => Support::Established,
                            };
                            Some(Hop {
                                to: fact.object.clone(),
                                fact: ids[i].clone(),
                                support,
                            })
                        })
                        .collect(),
                    end: RouteEnd::ReachedUse,
                },
                Trace::Stopped { reason } => Route {
                    use_site: Some(receiver_use.call.clone()),
                    origin: receiver_use
                        .moves
                        .iter()
                        .find_map(|id| moves.facts.get(id))
                        .map(|fact| fact.subject.clone())
                        .unwrap_or_else(|| {
                            Endpoint::Unresolved(format!("receiver of {}", receiver_use.call.0))
                        }),
                    hops: Vec::new(),
                    end: RouteEnd::Stopped {
                        at: receiver_use.call.clone(),
                        reason,
                    },
                },
            };
            routes.insert(
                RouteId(format!("value:{}:{index}", receiver_use.call.0)),
                route,
            );
        }
    }
    routes
}

fn is_move(kind: &FactKind) -> bool {
    matches!(
        kind,
        FactKind::Construct
            | FactKind::Store
            | FactKind::Read
            | FactKind::Pass
            | FactKind::Return
            | FactKind::Coerce
            | FactKind::Capture
    )
}

/// One-hop routes for each fact that shows one module using another or
/// something outside the project. `module_of` gives the module an endpoint
/// lies in, or `None` outside the project.
pub fn direct_routes(
    facts: &BTreeMap<FactId, Fact>,
    module_of: impl Fn(&Endpoint) -> Option<ModuleId>,
) -> BTreeMap<RouteId, Route> {
    facts
        .iter()
        .filter(|(_, fact)| is_direct_use(&fact.kind))
        .filter(|(_, fact)| {
            let user = module_of(&fact.subject);
            user.is_some() && user != module_of(&fact.object)
        })
        .map(|(id, fact)| {
            let route = Route {
                use_site: Some(id.clone()),
                origin: fact.object.clone(),
                hops: vec![Hop {
                    to: fact.subject.clone(),
                    fact: id.clone(),
                    support: Support::Established,
                }],
                end: RouteEnd::ReachedUse,
            };
            (RouteId(format!("use:{}", id.0)), route)
        })
        .collect()
}

fn is_direct_use(kind: &FactKind) -> bool {
    matches!(
        kind,
        FactKind::DeclaredDependency
            | FactKind::Import { .. }
            | FactKind::Reexport { .. }
            | FactKind::TypeReference
            | FactKind::ValueReference
            | FactKind::Implements
            | FactKind::Call(_)
    )
}

/// Receiver bounds for each call made through a contract: one per class
/// with an `Implements` fact for the contract that declares the called
/// method. A single implementer is unique in the build; several are each a
/// candidate. `declared_by` gives the type that declares a method.
pub fn receiver_bounds(
    facts: &BTreeMap<FactId, Fact>,
    declared_by: impl Fn(&ItemId) -> Option<ItemId>,
) -> BTreeMap<FactId, Fact> {
    let mut implementers: BTreeMap<&ItemId, BTreeSet<&ItemId>> = BTreeMap::new();
    for fact in facts.values() {
        if let (FactKind::Implements, Endpoint::Item(class), Endpoint::Item(contract)) =
            (&fact.kind, &fact.subject, &fact.object)
        {
            implementers.entry(contract).or_default().insert(class);
        }
    }
    let mut bounds = BTreeMap::new();
    for (call_id, call) in facts {
        let (FactKind::Call(project_graph::Dispatch::Contract), Endpoint::Item(method)) =
            (&call.kind, &call.object)
        else {
            continue;
        };
        let Some(contract) = declared_by(method) else {
            continue;
        };
        let Some(classes) = implementers.get(&contract) else {
            continue;
        };
        let basis = if classes.len() == 1 {
            ReceiverBasis::UniqueInClosedBuild
        } else {
            ReceiverBasis::BuildCandidate
        };
        for class in classes {
            bounds.insert(
                FactId(format!("receiver_bound:{}:{}", call_id.0, class.0)),
                Fact {
                    unit: call.unit.clone(),
                    subject: Endpoint::Place {
                        owner: method.clone(),
                        projection: "self".into(),
                    },
                    kind: FactKind::ReceiverBound {
                        call: call_id.clone(),
                        basis: basis.clone(),
                    },
                    object: Endpoint::Item((*class).clone()),
                    site: call.site.clone(),
                },
            );
        }
    }
    bounds
}

#[cfg(test)]
mod tests;
