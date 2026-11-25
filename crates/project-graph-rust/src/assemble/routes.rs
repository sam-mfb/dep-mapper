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
//! traced through but are not hops. Moves into or out of the toolchain's
//! own crates are not traced.
//!
//! A traced route is kept when its origin lies in another module than the
//! call. A call whose trait lies in another module, with no origin found,
//! gets a route that stops at the call.

use super::{Ordered, Resolver, Unit, Units, site};
use project_graph::{Dispatch, Endpoint, Fact, FactKind, ModuleId};
use project_graph_rust_records as r;
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
    let uses: Vec<Use> = receivers
        .iter()
        .map(|r| Use {
            receiver: r.receiver.clone(),
        })
        .collect();
    let traces = value_routes::trace(&flows, &uses, LIMITS);
    let mut routes = Vec::new();
    for (receiver, traces) in receivers.iter().zip(traces) {
        for trace in traces {
            match trace {
                Trace::Reached { origin, path, .. } => {
                    if resolve
                        .module(&origin)
                        .is_none_or(|m| Some(m) == receiver.owner_module)
                    {
                        continue;
                    }
                    routes.push(TracedRoute {
                        call: receiver.call.clone(),
                        origin,
                        hops: path
                            .iter()
                            .filter_map(|&i| {
                                Some((flows[i].fact.clone()?, flows[i].joins_instances))
                            })
                            .collect(),
                        stopped: None,
                    });
                }
                Trace::Stopped { reason } => {
                    if receiver.contract_module.is_none()
                        || receiver.contract_module == receiver.owner_module
                    {
                        continue;
                    }
                    routes.push(TracedRoute {
                        call: receiver.call.clone(),
                        origin: receiver.receiver.clone(),
                        hops: Vec::new(),
                        stopped: Some(reason),
                    });
                }
            }
        }
    }
    routes.sort_by(|a, b| {
        Ordered(a.call.clone())
            .cmp(&Ordered(b.call.clone()))
            .then_with(|| format!("{:?}", a.origin).cmp(&format!("{:?}", b.origin)))
    });
    routes
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
        let joins_instances = [&flow.from, &flow.to]
            .iter()
            .any(|n| matches!(n, r::Node::Field { .. } | r::Node::Capture { .. }));
        let kind = match flow.kind {
            r::FlowKind::Copy => None,
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
            owner_module,
            contract_module,
        })
    }
}
