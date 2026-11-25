//! Routes for calls left at a trait: which implementations reach the
//! receiver, and through which value moves.
//!
//! The driver's flow records become value moves for the `value-routes`
//! tracer, with endpoints named as in the rest of the graph:
//!
//! - a local, argument or returned value is a place in the function's item,
//!   such as `consume` `_3`, `backups _5` (a named variable keeps its index,
//!   since shadowed variables share a name), `argument 0` or `returned`,
//!   prefixed by the closure it belongs to, if any;
//! - a struct field is a place in the struct's item, named by the field. It
//!   stands for that field in every instance, so moves through it are
//!   possible, not established;
//! - a type being constructed or made a trait object is the origin.
//!
//! Moves the reader does not need, such as copies between temporaries, are
//! traced through but are not hops. A call into the standard library is
//! assumed to pass its arguments to its result; a hop that relies on that,
//! or on a place standing for every instance, is possible, not established.
//!
//! A traced route is kept when its origin lies in another module than the
//! call. A call whose trait lies in another module, with no origin found,
//! gets a route that stops at the call.

use super::{Ordered, Resolver, Unit, Units, site};
use project_graph::{Dispatch, Endpoint, Fact, FactKind, ModuleId};
use project_graph_rust_records as r;
use std::collections::{HashMap, HashSet, VecDeque};
use value_routes::{CallLink, Flow, Limits, Trace, Use};

const LIMITS: Limits = Limits {
    max_states: 20_000,
    max_call_depth: 8,
};

pub(super) struct TracedRoute {
    /// The call whose receiver this explains.
    pub call: Fact,
    pub origin: Endpoint,
    /// The reported moves in order, each marked when it joins instances.
    pub hops: Vec<(Fact, bool)>,
    pub stopped: Option<String>,
}

struct Receiver {
    call: Fact,
    receiver: Endpoint,
    owner_module: Option<ModuleId>,
    contract_module: Option<ModuleId>,
    /// The trait the call goes through, as the key of `Implementors`.
    contract_trait: (String, String),
}

/// For each trait implemented in the build, keyed by crate and path: the
/// implementing types, or `None` when a blanket implementation means any
/// type may implement it.
type Implementors = HashMap<(String, String), Option<Vec<Endpoint>>>;

fn trait_key(def: &r::Def) -> (String, String) {
    (def.crate_id.clone(), def.path.clone())
}

fn implementors(resolve: &Resolver, units: &Units, crates: &[Vec<r::Record>]) -> Implementors {
    let mut found: Implementors = HashMap::new();
    for (_, records) in units.iter(crates) {
        for record in records {
            let r::Record::Impl(implementation) = record else {
                continue;
            };
            let entry = found
                .entry(trait_key(&implementation.contract))
                .or_insert_with(|| Some(Vec::new()));
            let provider = implementation
                .self_type
                .as_ref()
                .and_then(|def| resolve.endpoint(def))
                .map(|(endpoint, _)| endpoint);
            match (entry.as_mut(), &implementation.self_type, provider) {
                (Some(types), _, Some(provider)) => types.push(provider),
                // A blanket or structural implementation: any type may.
                (Some(_), None, _) => *entry = None,
                // A type outside the project, such as `String`, cannot be an
                // origin of a traced route, so the list stays as it is.
                (Some(_), Some(_), None) | (None, _, _) => {}
            }
        }
    }
    found
}

pub(super) fn traced(
    resolve: &Resolver,
    units: &Units,
    crates: &[Vec<r::Record>],
) -> Vec<TracedRoute> {
    let mut flows = Vec::new();
    let mut receivers = Vec::new();
    for (unit, records) in units.iter(crates) {
        for record in records {
            match record {
                r::Record::Flow(flow) => flows.extend(resolve.at(unit).flow(unit, flow)),
                r::Record::Receiver(receiver) => {
                    receivers.extend(resolve.at(unit).receiver(unit, receiver))
                }
                _ => {}
            }
        }
    }
    flows.extend(closure_flows(resolve, units, crates));
    let uses: Vec<Use> = receivers
        .iter()
        .map(|r| Use {
            receiver: r.receiver.clone(),
        })
        .collect();
    let implementors = implementors(resolve, units, crates);
    let traces = value_routes::trace(&flows, &uses, LIMITS);
    let mut routes = Vec::new();
    for (receiver, traces) in receivers.iter().zip(traces) {
        let crosses_modules =
            receiver.contract_module.is_some() && receiver.contract_module != receiver.owner_module;
        let stop = |reason: String| TracedRoute {
            call: receiver.call.clone(),
            origin: receiver.receiver.clone(),
            hops: Vec::new(),
            stopped: Some(reason),
        };
        let mut implementer_reached = false;
        let mut stopped = false;
        for trace in traces {
            match trace {
                Trace::Reached { origin, path, .. } => {
                    // A type that does not implement the trait cannot be
                    // the receiver, however the approximate moves joined it.
                    let implements = match implementors.get(&receiver.contract_trait) {
                        Some(Some(types)) => types.contains(&origin),
                        Some(None) => true,
                        None => false,
                    };
                    if !implements {
                        continue;
                    }
                    implementer_reached = true;
                    if resolve
                        .module(&origin)
                        .is_none_or(|m| Some(m) == receiver.owner_module)
                    {
                        continue;
                    }
                    routes.push(TracedRoute {
                        call: receiver.call.clone(),
                        origin,
                        hops: hops(&flows, &path),
                        stopped: None,
                    });
                }
                Trace::Stopped { reason } => {
                    stopped = true;
                    if crosses_modules {
                        routes.push(stop(reason));
                    }
                }
            }
        }
        if crosses_modules && !implementer_reached && !stopped {
            routes.push(stop(
                "only types that do not implement the trait reach the receiver".to_string(),
            ));
        }
    }
    routes.sort_by(|a, b| {
        Ordered(a.call.clone())
            .cmp(&Ordered(b.call.clone()))
            .then_with(|| format!("{:?}", a.origin).cmp(&format!("{:?}", b.origin)))
    });
    routes
}

/// Moves into and out of closures called where the compiler did not say
/// which closure. For a call through a value of generic closure type, the
/// closures are those whose construction the recorded moves lead back to,
/// through parameters, captures and async bodies. For a closure passed to
/// the standard library, the call's other arguments are assumed to reach
/// it. Either way the moves join every call involved, so a hop that relies
/// on them is possible.
fn closure_flows(resolve: &Resolver, units: &Units, crates: &[Vec<r::Record>]) -> Vec<Flow> {
    let mut into: HashMap<r::Node, Vec<r::Node>> = HashMap::new();
    for (_, records) in units.iter(crates) {
        for record in records {
            if let r::Record::Flow(flow) = record {
                into.entry(identity(&flow.to))
                    .or_default()
                    .push(identity(&flow.from));
            }
        }
    }
    let through = |unit: &Unit, from: &r::Node, to: r::Node, span: &r::Span| {
        resolve.at(unit).flow(
            unit,
            &r::FlowRecord {
                from: from.clone(),
                to,
                kind: r::FlowKind::Through,
                call_site: None,
                span: Some(span.clone()),
            },
        )
    };
    let parameter = |closure: &r::Def, position: usize| r::Node::Param {
        // A closure's argument 0 is its captured environment; its own
        // parameters follow.
        function: closure.clone(),
        index: position as u32 + 1,
    };
    let returned = |closure: &r::Def| r::Node::Returned {
        function: closure.clone(),
    };
    let mut flows = Vec::new();
    for (unit, records) in units.iter(crates) {
        for record in records {
            match record {
                r::Record::ClosureCall(call) => {
                    for closure in closures_reaching(&into, &call.callable) {
                        // The arguments travel as one tuple, which stands
                        // for each of them.
                        if let Some(arguments) = &call.arguments {
                            for position in 0..call.arity as usize {
                                flows.extend(through(
                                    unit,
                                    arguments,
                                    parameter(closure, position),
                                    &call.span,
                                ));
                            }
                        }
                        flows.extend(through(
                            unit,
                            &returned(closure),
                            call.destination.clone(),
                            &call.span,
                        ));
                    }
                }
                r::Record::ClosureBinding(binding) => {
                    for (position, others) in binding.parameters.iter().enumerate() {
                        for other in others {
                            flows.extend(through(
                                unit,
                                other,
                                parameter(&binding.closure, position),
                                &binding.span,
                            ));
                        }
                    }
                    flows.extend(through(
                        unit,
                        &returned(&binding.closure),
                        binding.destination.clone(),
                        &binding.span,
                    ));
                }
                _ => {}
            }
        }
    }
    flows
}

/// The closures whose construction reaches `callable`, following recorded
/// moves backward without regard to calls. Bounded: a closure called
/// through a long chain of generic code may be missed.
fn closures_reaching<'a>(
    into: &'a HashMap<r::Node, Vec<r::Node>>,
    callable: &r::Node,
) -> Vec<&'a r::Def> {
    const MOST_NODES: usize = 2_000;
    let start = identity(callable);
    let mut seen: HashSet<&r::Node> = HashSet::new();
    let mut queue: VecDeque<&r::Node> = into.get(&start).into_iter().flatten().collect();
    let mut closures = Vec::new();
    while let Some(node) = queue.pop_front() {
        if seen.len() >= MOST_NODES {
            break;
        }
        if !seen.insert(node) {
            continue;
        }
        match node {
            r::Node::Type(def) => {
                if is_closure(def) && !closures.contains(&def) {
                    closures.push(def);
                }
            }
            _ => queue.extend(into.get(node).into_iter().flatten()),
        }
    }
    closures
}

/// A node with its definitions' display names removed: the same
/// definition is named differently from different crates.
fn identity(node: &r::Node) -> r::Node {
    let plain = |def: &r::Def| r::Def {
        name: String::new(),
        ..def.clone()
    };
    match node {
        r::Node::Local {
            function,
            local,
            name,
        } => r::Node::Local {
            function: plain(function),
            local: *local,
            name: name.clone(),
        },
        r::Node::Param { function, index } => r::Node::Param {
            function: plain(function),
            index: *index,
        },
        r::Node::Returned { function } => r::Node::Returned {
            function: plain(function),
        },
        r::Node::Field { owner, field } => r::Node::Field {
            owner: plain(owner),
            field: field.clone(),
        },
        r::Node::Capture { closure, index } => r::Node::Capture {
            closure: plain(closure),
            index: *index,
        },
        r::Node::Type(def) => r::Node::Type(plain(def)),
    }
}

fn is_closure(def: &r::Def) -> bool {
    def.path
        .rsplit("::")
        .next()
        .is_some_and(|last| last.starts_with("{closure#"))
}

/// The reported moves of a path, in order. A hop is possible when its own
/// move, or an unreported move since the previous hop, is an approximation:
/// a place standing for many instances, or a pass through the standard
/// library.
fn hops(flows: &[Flow], path: &[usize]) -> Vec<(Fact, bool)> {
    let mut hops: Vec<(Fact, bool)> = Vec::new();
    let mut approximate = false;
    for &index in path {
        approximate |= flows[index].joins_instances;
        if let Some(fact) = &flows[index].fact {
            hops.push((fact.clone(), approximate));
            approximate = false;
        }
    }
    if approximate && let Some(last) = hops.last_mut() {
        last.1 = true;
    }
    hops
}

impl Resolver<'_> {
    /// The project module an endpoint belongs to.
    fn module(&self, endpoint: &Endpoint) -> Option<ModuleId> {
        match endpoint {
            Endpoint::Item(id) | Endpoint::Place { owner: id, .. } => {
                self.items.get(id).map(|i| i.module.clone())
            }
            Endpoint::Module(module) => Some(module.clone()),
            Endpoint::External(_) | Endpoint::Unresolved(_) => None,
        }
    }

    /// A place labelled `label` inside the function or closure `def`.
    fn place_in(&self, def: &r::Def, label: &str) -> Option<Endpoint> {
        let (Endpoint::Item(owner), _) = self.endpoint(def)? else {
            return None;
        };
        let full = self.units.item_id(def).0;
        let closure = full[owner.0.len()..].trim_start_matches("::");
        let projection = if closure.is_empty() {
            label.to_string()
        } else {
            format!("{closure} {label}")
        };
        Some(Endpoint::Place { owner, projection })
    }

    fn node(&self, node: &r::Node) -> Option<Endpoint> {
        match node {
            r::Node::Type(def) => match self.endpoint(def)? {
                (endpoint @ Endpoint::Item(_), _) => Some(endpoint),
                _ => None,
            },
            r::Node::Field { owner, field } => match self.endpoint(owner)? {
                (Endpoint::Item(owner), _) => Some(Endpoint::Place {
                    owner,
                    projection: field.clone(),
                }),
                _ => None,
            },
            r::Node::Capture { closure, index } => {
                self.place_in(closure, &format!("capture {index}"))
            }
            r::Node::Local {
                function,
                local,
                name,
            } => {
                // Shadowed variables share a name: the index tells them apart.
                let label = match name {
                    Some(name) => format!("{name} _{local}"),
                    None => format!("_{local}"),
                };
                self.place_in(function, &label)
            }
            r::Node::Param { function, index } => {
                self.place_in(function, &format!("argument {index}"))
            }
            r::Node::Returned { function } => self.place_in(function, "returned"),
        }
    }

    fn flow(&self, unit: &Unit, flow: &r::FlowRecord) -> Option<Flow> {
        let from = self.node(&flow.from)?;
        let to = self.node(&flow.to)?;
        let joins_instances = flow.kind == r::FlowKind::Through
            || [&flow.from, &flow.to]
                .iter()
                .any(|n| matches!(n, r::Node::Field { .. } | r::Node::Capture { .. }));
        let kind = match flow.kind {
            r::FlowKind::Copy | r::FlowKind::Through => None,
            r::FlowKind::Construct => Some(FactKind::Construct),
            r::FlowKind::Coerce => Some(FactKind::Coerce),
            r::FlowKind::Store => Some(FactKind::Store),
            r::FlowKind::Read => Some(FactKind::Read),
            r::FlowKind::Pass => Some(FactKind::Pass),
            r::FlowKind::Return => Some(FactKind::Return),
            r::FlowKind::Capture => Some(FactKind::Capture),
        };
        let call = flow.call_site.clone().and_then(|s| match flow.kind {
            r::FlowKind::Pass | r::FlowKind::Construct
                if matches!(flow.to, r::Node::Param { .. }) =>
            {
                Some(CallLink::Argument(s))
            }
            r::FlowKind::Return => Some(CallLink::Returned(s)),
            _ => None,
        });
        Some(Flow {
            fact: kind.map(|kind| Fact {
                unit: unit.id.clone(),
                subject: from.clone(),
                kind,
                object: to.clone(),
                site: flow.span.as_ref().map(|s| site(s, &self.workspace.root)),
            }),
            from,
            to,
            call,
            joins_instances,
        })
    }

    fn receiver(&self, unit: &Unit, receiver: &r::ReceiverRecord) -> Option<Receiver> {
        let (owner, owner_module) = self.endpoint(&receiver.owner)?;
        let (contract, contract_module) = self.endpoint(&receiver.contract)?;
        Some(Receiver {
            call: Fact {
                unit: unit.id.clone(),
                subject: owner,
                kind: FactKind::Call(Dispatch::Contract),
                object: contract,
                site: Some(site(&receiver.span, &self.workspace.root)),
            },
            receiver: self.node(&receiver.receiver)?,
            contract_trait: trait_key(&receiver.contract_trait),
            owner_module,
            contract_module,
        })
    }
}
