//! Routes over a graph's facts. Nothing here depends on TypeScript: the input
//! is `project_graph` facts, so this module can move to a library shared
//! with other languages.
//!
//! - A direct use between two modules, such as an import or a call, is a
//!   route of one hop from the supplier to the user.
//! - A value route follows value moves backward from a method call's
//!   receiver to the place the value was constructed, then lists the hops
//!   forward. A trace that finds no further move ends as a stopped route.
//! - A receiver bound names each class that implements the contract a call
//!   is made through.

use project_graph::{
    CoverageGap, Endpoint, Fact, FactId, FactKind, Hop, ItemId, ModuleId, ReceiverBasis, Route,
    RouteEnd, RouteId, Support,
};
use std::collections::{BTreeMap, BTreeSet};

/// A method call and the value moves that deliver its receiver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverUse {
    pub call: FactId,
    pub moves: Vec<FactId>,
}

/// Bounds on the backward search for each use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceLimits {
    /// The most hops one route may have before the trace stops.
    pub max_hops: usize,
    /// The most routes kept for one use; the rest are reported as a gap.
    pub max_routes_per_use: usize,
}

impl TraceLimits {
    /// Limits that cover every route in small and medium projects.
    pub const STANDARD: TraceLimits = TraceLimits {
        max_hops: 32,
        max_routes_per_use: 16,
    };
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Traced {
    pub routes: BTreeMap<RouteId, Route>,
    pub gaps: Vec<CoverageGap>,
}

/// Value routes for each receiver use. `conditions` holds the condition of
/// each move that happens on only one branch; its hop is possible, not
/// established.
pub fn trace_values(
    facts: &BTreeMap<FactId, Fact>,
    conditions: &BTreeMap<FactId, String>,
    uses: &[ReceiverUse],
    limits: TraceLimits,
) -> Traced {
    let mut incoming: BTreeMap<String, Vec<&FactId>> = BTreeMap::new();
    for (id, fact) in facts {
        if is_move(&fact.kind) {
            incoming.entry(key(&fact.object)).or_default().push(id);
        }
    }
    let search = Search {
        facts,
        conditions,
        incoming,
        limits,
    };
    let mut traced = Traced::default();
    for receiver in uses {
        let mut found = Vec::new();
        for start in &receiver.moves {
            if facts.contains_key(start) {
                search.backward(&mut vec![start.clone()], &mut found);
            }
        }
        if found.len() > limits.max_routes_per_use {
            traced.gaps.push(CoverageGap {
                unit: facts.get(&receiver.call).map(|f| f.unit.clone()),
                site: facts.get(&receiver.call).and_then(|f| f.site.clone()),
                reason: format!(
                    "{} value routes reach this call; only the first {} are kept",
                    found.len(),
                    limits.max_routes_per_use
                ),
            });
            found.truncate(limits.max_routes_per_use);
        }
        for (index, (origin, hops, end)) in found.into_iter().enumerate() {
            traced.routes.insert(
                RouteId(format!("value:{}:{index}", receiver.call.0)),
                Route {
                    use_site: Some(receiver.call.clone()),
                    origin,
                    hops,
                    end,
                },
            );
        }
    }
    traced
}

struct Search<'a> {
    facts: &'a BTreeMap<FactId, Fact>,
    conditions: &'a BTreeMap<FactId, String>,
    incoming: BTreeMap<String, Vec<&'a FactId>>,
    limits: TraceLimits,
}

type Found = (Endpoint, Vec<Hop>, RouteEnd);

impl Search<'_> {
    /// `path` holds the moves found so far, the latest (earliest in the
    /// value's journey) last.
    fn backward(&self, path: &mut Vec<FactId>, found: &mut Vec<Found>) {
        let earliest = path.last().expect("a path starts with one move");
        let fact = &self.facts[earliest];
        let source = &fact.subject;
        if fact.kind == FactKind::Construct || matches!(source, Endpoint::External(_)) {
            found.push((source.clone(), self.hops(path), RouteEnd::ReachedUse));
            return;
        }
        if path.len() >= self.limits.max_hops {
            found.push(self.stopped(path, "the search reached its hop limit".into()));
            return;
        }
        let entering: Vec<&FactId> = self
            .incoming
            .get(&key(source))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| !path.contains(id))
            .collect();
        if entering.is_empty() {
            let reason = match source {
                Endpoint::Place { projection, .. } if projection == "this" => {
                    "the value is the object whose method holds this code".to_string()
                }
                _ => format!("no recorded value enters {}", describe(source)),
            };
            found.push(self.stopped(path, reason));
            return;
        }
        for id in entering {
            path.push(id.clone());
            self.backward(path, found);
            path.pop();
        }
    }

    fn stopped(&self, path: &[FactId], reason: String) -> Found {
        let earliest = path.last().expect("a path starts with one move");
        (
            self.facts[earliest].subject.clone(),
            self.hops(path),
            RouteEnd::Stopped {
                at: earliest.clone(),
                reason,
            },
        )
    }

    fn hops(&self, path: &[FactId]) -> Vec<Hop> {
        path.iter()
            .rev()
            .map(|id| Hop {
                to: self.facts[id].object.clone(),
                fact: id.clone(),
                support: match self.conditions.get(id) {
                    Some(condition) => Support::Possible {
                        condition: Some(condition.clone()),
                    },
                    None => Support::Established,
                },
            })
            .collect()
    }
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

/// A key that identifies an endpoint for lookups.
fn key(endpoint: &Endpoint) -> String {
    format!("{endpoint:?}")
}

fn describe(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Module(ModuleId(id)) | Endpoint::Item(ItemId(id)) => id.clone(),
        Endpoint::Place { owner, projection } => format!("{projection} in {}", owner.0),
        Endpoint::External(name) | Endpoint::Unresolved(name) => name.clone(),
    }
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
