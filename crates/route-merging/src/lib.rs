//! Merges routes that follow the same path, so a graph has one route per
//! distinct path rather than one per use site.
//!
//! - Two routes follow the same path when they have the same origin, the
//!   same hops in order, each going to the same endpoint with the same
//!   support, and the same end: both reach their use, or both stop for the
//!   same reason.
//! - The merged route names every use site of the routes it replaces. Each
//!   of its hops cites every fact behind that hop in those routes, and a
//!   stopped end cites every place they stopped. Each list keeps the order
//!   of first appearance and names a fact once.
//! - The merged route keeps the first route ID of its group, in ID order.
//! - Routes whose paths differ in any step stay separate. Facts are not
//!   touched.

use project_graph::{Endpoint, FactId, Route, RouteEnd, RouteId, Support};
use std::collections::{BTreeMap, BTreeSet};

/// The routes of `routes`, with those that follow the same path merged as
/// the crate documentation describes.
pub fn merge(routes: BTreeMap<RouteId, Route>) -> BTreeMap<RouteId, Route> {
    // `routes` iterates in ID order, so each group's first route comes first.
    let mut groups: BTreeMap<PathKey, Vec<(RouteId, Route)>> = BTreeMap::new();
    for (id, route) in routes {
        groups
            .entry(PathKey::of(&route))
            .or_default()
            .push((id, route));
    }
    groups.into_values().filter_map(combine).collect()
}

/// What two routes must share to merge. Endpoints and support have no
/// order of their own, so the key compares their full debug forms.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PathKey {
    origin: String,
    hops: Vec<(String, String)>,
    /// `None` for a route that reached its use, or the reason it stopped.
    stopped: Option<String>,
}

impl PathKey {
    fn of(route: &Route) -> Self {
        PathKey {
            origin: debug(&route.origin),
            hops: route
                .hops
                .iter()
                .map(|hop| (debug(&hop.to), support(&hop.support)))
                .collect(),
            stopped: match &route.end {
                RouteEnd::ReachedUse => None,
                RouteEnd::Stopped { reason, .. } => Some(reason.clone()),
            },
        }
    }
}

fn debug(endpoint: &Endpoint) -> String {
    format!("{endpoint:?}")
}

fn support(support: &Support) -> String {
    format!("{support:?}")
}

/// One route for a group that follows the same path, under the group's
/// first ID. `None` only for an empty group.
fn combine(group: Vec<(RouteId, Route)>) -> Option<(RouteId, Route)> {
    let mut group = group.into_iter();
    let (id, mut into) = group.next()?;
    let mut use_sites = Facts::new(std::mem::take(&mut into.use_sites));
    let mut hops: Vec<Facts> = into
        .hops
        .iter_mut()
        .map(|hop| Facts::new(std::mem::take(&mut hop.facts)))
        .collect();
    let mut stopped_at = match &mut into.end {
        RouteEnd::Stopped { at, .. } => Facts::new(std::mem::take(at)),
        RouteEnd::ReachedUse => Facts::new(Vec::new()),
    };
    for (_, route) in group {
        use_sites.extend(route.use_sites);
        for (facts, hop) in hops.iter_mut().zip(route.hops) {
            facts.extend(hop.facts);
        }
        if let RouteEnd::Stopped { at, .. } = route.end {
            stopped_at.extend(at);
        }
    }
    into.use_sites = use_sites.list;
    for (hop, facts) in into.hops.iter_mut().zip(hops) {
        hop.facts = facts.list;
    }
    if let RouteEnd::Stopped { at, .. } = &mut into.end {
        *at = stopped_at.list;
    }
    Some((id, into))
}

/// A list of facts in order of first appearance, each named once.
struct Facts {
    list: Vec<FactId>,
    seen: BTreeSet<FactId>,
}

impl Facts {
    fn new(list: Vec<FactId>) -> Self {
        let mut facts = Facts {
            list: Vec::new(),
            seen: BTreeSet::new(),
        };
        facts.extend(list);
        facts
    }

    fn extend(&mut self, more: Vec<FactId>) {
        for fact in more {
            if self.seen.insert(fact.clone()) {
                self.list.push(fact);
            }
        }
    }
}

#[cfg(test)]
mod tests;
