//! The map's routes: each graph route with its hops placed on the map, its
//! support and end kept as the graph records them, and its review.

use crate::BuildProblem;
use crate::citations::Citations;
use crate::dependencies::Dependencies;
use crate::points::{item_of, module_of, outside_point, project_point, route_point};
use crate::reviews::Reviews;
use project_graph::{FactId, FactKind, GraphRef, ProjectGraph, Route, RouteEnd, RouteId, Support};
use project_map::{CitationId, HopSupport, HopTarget, MapHop, MapRoute, MapRouteEnd, Status};
use std::collections::BTreeMap;

/// Every route in the graph. Records each hop's fact as evidence for the
/// dependency the hop travels along, and reports references the graph
/// cannot resolve.
pub(crate) fn build<'g>(
    graph: &'g ProjectGraph,
    reviews: &Reviews,
    dependencies: &mut Dependencies,
    citations: &mut Citations<'g>,
    problems: &mut Vec<BuildProblem>,
) -> BTreeMap<project_map::RouteId, MapRoute> {
    graph
        .routes
        .iter()
        .map(|(id, route)| {
            let mut builder = RouteBuilder {
                graph,
                id,
                citations: &mut *citations,
                problems: &mut *problems,
            };
            let map_route = builder.build(route, reviews, dependencies);
            (project_map::RouteId(id.0.clone()), map_route)
        })
        .collect()
}

struct RouteBuilder<'a, 'g> {
    graph: &'g ProjectGraph,
    id: &'g RouteId,
    citations: &'a mut Citations<'g>,
    problems: &'a mut Vec<BuildProblem>,
}

impl RouteBuilder<'_, '_> {
    fn build(
        &mut self,
        route: &Route,
        reviews: &Reviews,
        dependencies: &mut Dependencies,
    ) -> MapRoute {
        let graph = self.graph;
        self.check_item(&route.origin);
        let mut sender = module_of(graph, &route.origin);
        let mut hops = vec![];
        for hop in &route.hops {
            self.check_item(&hop.to);
            let evidence: Vec<CitationId> = hop.facts.iter().map(|id| self.fact(id)).collect();
            let to = match (module_of(graph, &hop.to), project_point(graph, &hop.to)) {
                (Some(receiver), Some(point)) => {
                    let kinds: Vec<&FactKind> = hop
                        .facts
                        .iter()
                        .filter_map(|id| graph.facts.get(id))
                        .map(|fact| &fact.kind)
                        .collect();
                    let link = dependencies.link(sender, receiver, &kinds, &evidence);
                    HopTarget::Project { point, link }
                }
                _ => HopTarget::Outside(outside_point(graph, &hop.to)),
            };
            sender = module_of(graph, &hop.to);
            hops.push(MapHop {
                to,
                role: reviews.for_hop(&evidence),
                evidence,
                support: match &hop.support {
                    Support::Established => HopSupport::Established,
                    Support::Possible { condition } => HopSupport::Possible {
                        condition: condition.clone(),
                    },
                },
            });
        }
        let end = match &route.end {
            RouteEnd::ReachedUse => MapRouteEnd::ReachedUse,
            RouteEnd::Stopped { at, reason } => MapRouteEnd::Stopped {
                at: at.iter().map(|id| self.fact(id)).collect(),
                reason: reason.clone(),
            },
        };
        MapRoute {
            status: Status::Observed,
            use_sites: route.use_sites.iter().map(|id| self.fact(id)).collect(),
            origin: route_point(graph, &route.origin),
            hops,
            end,
            description: reviews.for_route(self.id),
        }
    }

    /// The citation of a fact the route refers to, reporting it if missing.
    fn fact(&mut self, id: &FactId) -> CitationId {
        let (citation, held) = self.citations.fact(id);
        if !held {
            self.problems.push(BuildProblem::IncompleteGraph {
                cited_by: GraphRef::from(self.id.clone()),
                missing: GraphRef::from(id.clone()),
            });
        }
        citation
    }

    /// Reports an endpoint naming an item the graph does not hold.
    fn check_item(&mut self, endpoint: &project_graph::Endpoint) {
        if let Some(item) = item_of(endpoint).filter(|item| !self.graph.items.contains_key(item)) {
            self.problems.push(BuildProblem::IncompleteGraph {
                cited_by: GraphRef::from(self.id.clone()),
                missing: GraphRef::from(item.clone()),
            });
        }
    }
}
