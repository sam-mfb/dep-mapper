//! Leaves out of a `ProjectGraph` the routes that start in named outside
//! packages, and records in the graph what it left out.
//!
//! - A route is left out when its origin is an outside endpoint in one of the
//!   packages. The caller says which outside names lie in them, since each
//!   language names outside endpoints its own way.
//! - A fact is left out when only those routes cite it, unless both of its
//!   endpoints lie in the project. A fact between project endpoints stays,
//!   even when the route that cited it passed through an excluded package.
//! - A fact that another remaining fact cites, as a receiver bound cites its
//!   call, stays.
//! - Facts no route cites, items, modules and gaps are kept as they are.
//! - When the list of packages is not empty, the graph gains a coverage gap
//!   naming the packages and how many routes and facts were left out, even
//!   when none were. An empty list leaves the graph unchanged.

use project_graph::{CoverageGap, Endpoint, FactId, FactKind, ProjectGraph, Route, RouteEnd};
use std::collections::BTreeSet;

/// How much one exclusion left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Removed {
    pub routes: usize,
    pub facts: usize,
}

/// Leaves out of `graph` the routes that start in `packages`, and the facts
/// only they cite, as the module documentation describes. `in_packages`
/// says whether the outside endpoint with the given name lies in one of
/// `packages`.
pub fn exclude(
    graph: &mut ProjectGraph,
    packages: &[String],
    in_packages: impl Fn(&str) -> bool,
) -> Removed {
    if packages.is_empty() {
        return Removed {
            routes: 0,
            facts: 0,
        };
    }
    let (dropped, kept): (Vec<_>, Vec<_>) =
        std::mem::take(&mut graph.routes).into_iter().partition(
            |(_, route)| matches!(&route.origin, Endpoint::External(name) if in_packages(name)),
        );
    let cited_by_kept: BTreeSet<FactId> = kept.iter().flat_map(|(_, r)| cited(r)).collect();
    let mut removable: BTreeSet<FactId> = dropped
        .iter()
        .flat_map(|(_, r)| cited(r))
        .filter(|id| !cited_by_kept.contains(id))
        .filter(|id| {
            graph
                .facts
                .get(id)
                .is_some_and(|f| !(in_project(&f.subject) && in_project(&f.object)))
        })
        .collect();
    // A remaining fact keeps the facts it cites, and those keep theirs.
    loop {
        let needed: Vec<FactId> = graph
            .facts
            .iter()
            .filter(|(id, _)| !removable.contains(*id))
            .filter_map(|(_, fact)| match &fact.kind {
                FactKind::ReceiverBound { call, .. } if removable.contains(call) => {
                    Some(call.clone())
                }
                _ => None,
            })
            .collect();
        if needed.is_empty() {
            break;
        }
        for id in needed {
            removable.remove(&id);
        }
    }
    graph.facts.retain(|id, _| !removable.contains(id));
    graph.routes = kept.into_iter().collect();
    let removed = Removed {
        routes: dropped.len(),
        facts: removable.len(),
    };
    graph.gaps.push(CoverageGap {
        unit: None,
        site: None,
        reason: format!(
            "Routes that start in the excluded packages {} are left out: {} and {} removed.",
            packages.join(", "),
            count(removed.routes, "route", "routes"),
            count(removed.facts, "fact", "facts"),
        ),
    });
    removed
}

/// The facts a route cites.
fn cited(route: &Route) -> impl Iterator<Item = FactId> + '_ {
    let stopped_at = match &route.end {
        RouteEnd::Stopped { at, .. } => at.as_slice(),
        RouteEnd::ReachedUse => &[],
    };
    route
        .use_sites
        .iter()
        .chain(route.hops.iter().flat_map(|hop| hop.facts.iter()))
        .chain(stopped_at)
        .cloned()
}

fn in_project(endpoint: &Endpoint) -> bool {
    match endpoint {
        Endpoint::Module(_) | Endpoint::Item(_) | Endpoint::Place { .. } => true,
        Endpoint::External(_) | Endpoint::Unresolved(_) => false,
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests;
