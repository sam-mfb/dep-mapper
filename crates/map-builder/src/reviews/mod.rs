//! Matching the evaluation's relationships to routes, hops and dependencies,
//! and combining those that apply to one subject into its review.

use crate::BuildProblem;
use crate::citations::{Citations, route_citation};
use graph_evaluator::{GraphEvaluation, RelationshipKind};
use project_map::{
    CitationId, DependencyId, MapDependency, MapRoute, Review, RouteDescription, RouteId, UseRole,
};
use std::collections::BTreeMap;

pub(crate) struct Reviews {
    relationships: Vec<Placed>,
}

/// A relationship with its citations in map terms.
struct Placed {
    role: UseRole,
    reason: String,
    citations: Vec<CitationId>,
}

impl Reviews {
    /// Resolves each relationship's citations, reporting those the graph
    /// does not hold.
    pub(crate) fn new(
        evaluation: &GraphEvaluation,
        citations: &mut Citations,
        problems: &mut Vec<BuildProblem>,
    ) -> Self {
        let relationships = evaluation
            .relationships
            .iter()
            .enumerate()
            .map(|(index, relationship)| {
                let mut cited = vec![];
                for reference in &relationship.evidence {
                    match citations.cite(reference) {
                        Some(citation) if !cited.contains(&citation) => cited.push(citation),
                        Some(_) => {}
                        None => problems.push(BuildProblem::UnresolvedRelationshipCitation {
                            relationship: index,
                            reference: reference.clone(),
                        }),
                    }
                }
                Placed {
                    role: role(&relationship.kind),
                    reason: relationship.reason.clone(),
                    citations: cited,
                }
            })
            .collect();
        Reviews { relationships }
    }

    /// The review of the route with this graph route ID.
    pub(crate) fn for_route(&self, id: &project_graph::RouteId) -> Review<RouteDescription> {
        let citation = route_citation(id);
        self.combine(|placed| placed.citations.contains(&citation))
    }

    /// The role of the first relationship citing one of the hop's facts.
    pub(crate) fn for_hop(&self, facts: &[CitationId]) -> Option<UseRole> {
        self.relationships
            .iter()
            .find(|placed| cites_any(&placed.citations, facts))
            .map(|placed| placed.role.clone())
    }

    /// The review of a dependency, given its evidence and the citations of
    /// the routes that travel along it alone.
    pub(crate) fn for_dependency(
        &self,
        evidence: &[CitationId],
        routes: &[CitationId],
    ) -> Review<RouteDescription> {
        self.combine(|placed| {
            placed
                .citations
                .iter()
                .any(|citation| evidence.contains(citation) || routes.contains(citation))
        })
    }

    /// Relationships that apply to no route, hop or dependency on the map.
    /// A hop takes only the first relationship citing one of its facts, so a later
    /// one that reaches nothing else is reported too.
    pub(crate) fn unplaced(
        &self,
        routes: &BTreeMap<RouteId, MapRoute>,
        dependencies: &BTreeMap<DependencyId, MapDependency>,
    ) -> Vec<BuildProblem> {
        let mut subjects: Vec<CitationId> = vec![];
        let mut placed = vec![false; self.relationships.len()];
        for (id, route) in routes {
            subjects.push(route_citation(&project_graph::RouteId(id.0.clone())));
            for hop in &route.hops {
                let first = self
                    .relationships
                    .iter()
                    .position(|relationship| cites_any(&relationship.citations, &hop.evidence));
                if let Some(index) = first {
                    placed[index] = true;
                }
            }
        }
        for dependency in dependencies.values() {
            subjects.extend(dependency.evidence.iter().cloned());
        }
        self.relationships
            .iter()
            .enumerate()
            .filter(|(index, relationship)| {
                !placed[*index] && !relationship.citations.iter().any(|c| subjects.contains(c))
            })
            .map(|(relationship, _)| BuildProblem::UnplacedRelationship { relationship })
            .collect()
    }

    /// One review from every relationship that applies, in evaluation order.
    fn combine(&self, applies: impl Fn(&Placed) -> bool) -> Review<RouteDescription> {
        let applying: Vec<&Placed> = self.relationships.iter().filter(|p| applies(p)).collect();
        let Some((first, rest)) = applying.split_first() else {
            return Review::Unreviewed;
        };
        let mut citations: Vec<CitationId> = vec![];
        for citation in applying.iter().flat_map(|placed| &placed.citations) {
            if !citations.contains(citation) {
                citations.push(citation.clone());
            }
        }
        let primary = citations.remove(0);
        let mut other_roles: Vec<UseRole> = vec![];
        for placed in rest {
            if placed.role != first.role && !other_roles.contains(&placed.role) {
                other_roles.push(placed.role.clone());
            }
        }
        Review::Judged {
            judgment: RouteDescription {
                role: first.role.clone(),
                other_roles,
            },
            reason: applying
                .iter()
                .map(|placed| placed.reason.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            primary,
            supporting: citations,
        }
    }
}

fn role(kind: &RelationshipKind) -> UseRole {
    match kind {
        RelationshipKind::DirectUse => UseRole::DirectUse,
        RelationshipKind::Implements => UseRole::Implements,
        RelationshipKind::Wires => UseRole::Wires,
        RelationshipKind::SuppliesInput => UseRole::SuppliesInput,
        RelationshipKind::FullInjection => UseRole::FullInjection,
        RelationshipKind::AdaptedInjection => UseRole::AdaptedInjection,
        RelationshipKind::Incidental => UseRole::Incidental,
        RelationshipKind::Other(name) => UseRole::Other(name.clone()),
    }
}

/// Whether `citations` include any of `facts`.
fn cites_any(citations: &[CitationId], facts: &[CitationId]) -> bool {
    facts.iter().any(|fact| citations.contains(fact))
}
