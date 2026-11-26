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
//!
//! Before merging, a route that never leaves one project module ends at the
//! module rather than at the items inside it. A route qualifies when every
//! hop goes to an endpoint in the same project module, its origin lies
//! outside the project or in that module, and it reaches its use. Each of its
//! hops then goes to the module itself, keeping its facts and support, so
//! uses of one outside item anywhere in a module fold into one route. A
//! route that crosses modules, or that stops, keeps its detail.

use project_graph::{Endpoint, FactId, Item, ItemId, ModuleId, Route, RouteEnd, RouteId, Support};
use std::collections::{BTreeMap, BTreeSet};

/// The routes of `routes`, with those that stay in one module ending at the
/// module and those that follow the same path merged, as the crate
/// documentation describes. `items` are the graph's items, which say the
/// module each item endpoint lies in.
pub fn merge(
    routes: BTreeMap<RouteId, Route>,
    items: &BTreeMap<ItemId, Item>,
) -> BTreeMap<RouteId, Route> {
    // `routes` iterates in ID order, so each group's first route comes first.
    let mut groups: BTreeMap<PathKey, Vec<(RouteId, Route)>> = BTreeMap::new();
    for (id, route) in routes {
        let route = coarsen(route, items);
        groups
            .entry(PathKey::of(&route))
            .or_default()
            .push((id, route));
    }
    groups.into_values().filter_map(combine).collect()
}

/// `route` with each hop going to its module, when the route stays in one
/// project module and reaches its use; otherwise `route` unchanged.
fn coarsen(mut route: Route, items: &BTreeMap<ItemId, Item>) -> Route {
    let Some(module) = local_module(&route, items) else {
        return route;
    };
    for hop in &mut route.hops {
        hop.to = Endpoint::Module(module.clone());
    }
    route
}

/// The one project module a route stays in, if it qualifies.
fn local_module(route: &Route, items: &BTreeMap<ItemId, Item>) -> Option<ModuleId> {
    if route.end != RouteEnd::ReachedUse {
        return None;
    }
    let (first, rest) = route.hops.split_first()?;
    let module = module_of(&first.to, items)?;
    let hops_stay = rest
        .iter()
        .all(|hop| module_of(&hop.to, items).as_ref() == Some(&module));
    let origin_fits = match &route.origin {
        Endpoint::External(_) | Endpoint::Unresolved(_) => true,
        origin => module_of(origin, items).as_ref() == Some(&module),
    };
    (hops_stay && origin_fits).then_some(module)
}

/// The project module an endpoint lies in, or `None` outside the project.
fn module_of(endpoint: &Endpoint, items: &BTreeMap<ItemId, Item>) -> Option<ModuleId> {
    match endpoint {
        Endpoint::Module(module) => Some(module.clone()),
        Endpoint::Item(id) | Endpoint::Place { owner: id, .. } => {
            items.get(id).map(|item| item.module.clone())
        }
        Endpoint::External(_) | Endpoint::Unresolved(_) => None,
    }
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
