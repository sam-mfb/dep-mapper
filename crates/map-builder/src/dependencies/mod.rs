//! Module-to-module dependencies, found from the graph's use facts and from
//! the hops of its routes. The crate documentation gives the rules.

use crate::BuildProblem;
use crate::citations::{fact_citation, route_citation};
use crate::points::{item_of, module_of};
use crate::reviews::Reviews;
use project_graph::{FactKind, GraphRef, ModuleId, ProjectGraph};
use project_map::{CitationId, DependencyId, HopLink, MapDependency, MapRoute, RouteId, Status};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Dependencies {
    found: BTreeMap<DependencyId, Vec<CitationId>>,
}

impl Dependencies {
    /// The dependencies the graph's use facts establish. Reports facts that
    /// name items the graph does not hold.
    pub(crate) fn from_facts(graph: &ProjectGraph, problems: &mut Vec<BuildProblem>) -> Self {
        let mut dependencies = Dependencies {
            found: BTreeMap::new(),
        };
        for (id, fact) in &graph.facts {
            for endpoint in [&fact.subject, &fact.object] {
                if let Some(item) = item_of(endpoint).filter(|item| !graph.items.contains_key(item))
                {
                    problems.push(BuildProblem::IncompleteGraph {
                        cited_by: GraphRef::from(id.clone()),
                        missing: GraphRef::from(item.clone()),
                    });
                }
            }
            if !subject_uses_object(&fact.kind) {
                continue;
            }
            let (Some(user), Some(used)) = (
                module_of(graph, &fact.subject),
                module_of(graph, &fact.object),
            ) else {
                continue;
            };
            if user != used {
                dependencies.add(dependency(user, used), fact_citation(id));
            }
        }
        dependencies
    }

    /// How a hop from `sender` into `receiver` got there, recording the hop's
    /// facts as evidence for the dependency it travels along. `kinds` are the
    /// kinds of the hop's facts that the graph holds; when no dependency
    /// joins the modules yet, a read or a return among them makes the
    /// receiver use the sender.
    pub(crate) fn link(
        &mut self,
        sender: Option<&ModuleId>,
        receiver: &ModuleId,
        kinds: &[&FactKind],
        citations: &[CitationId],
    ) -> HopLink {
        let Some(sender) = sender else {
            return HopLink::FromOutside;
        };
        if sender == receiver {
            return HopLink::SameModule;
        }
        let receiver_uses = dependency(receiver, sender);
        let sender_uses = dependency(sender, receiver);
        let (link, id) = if self.found.contains_key(&receiver_uses) {
            (HopLink::ReceiverUsesSender, receiver_uses)
        } else if self.found.contains_key(&sender_uses) {
            (HopLink::SenderUsesReceiver, sender_uses)
        } else if kinds
            .iter()
            .any(|kind| matches!(kind, FactKind::Read | FactKind::Return))
        {
            (HopLink::ReceiverUsesSender, receiver_uses)
        } else {
            (HopLink::SenderUsesReceiver, sender_uses)
        };
        for citation in citations {
            self.add(id.clone(), citation.clone());
        }
        link
    }

    /// The map's dependencies, each reviewed by the relationships that apply
    /// to it.
    pub(crate) fn into_map(
        self,
        routes: &BTreeMap<RouteId, MapRoute>,
        reviews: &Reviews,
    ) -> BTreeMap<DependencyId, MapDependency> {
        let mut single: BTreeMap<DependencyId, Vec<CitationId>> = BTreeMap::new();
        for (id, route) in routes {
            let travelled: BTreeSet<DependencyId> = (0..route.hops.len())
                .filter_map(|hop| route.hop_dependency(hop))
                .collect();
            if let [only] = Vec::from_iter(travelled).as_slice() {
                single
                    .entry(only.clone())
                    .or_default()
                    .push(route_citation(&project_graph::RouteId(id.0.clone())));
            }
        }
        self.found
            .into_iter()
            .map(|(id, evidence)| {
                let routes = single.remove(&id).unwrap_or_default();
                let description = reviews.for_dependency(&evidence, &routes);
                let dependency = MapDependency {
                    status: Status::Observed,
                    evidence,
                    description,
                };
                (id, dependency)
            })
            .collect()
    }

    fn add(&mut self, id: DependencyId, citation: CitationId) {
        let evidence = self.found.entry(id).or_default();
        if !evidence.contains(&citation) {
            evidence.push(citation);
        }
    }
}

/// Whether the fact records its subject using its object, rather than a
/// value moving from one to the other.
fn subject_uses_object(kind: &FactKind) -> bool {
    match kind {
        FactKind::DeclaredDependency
        | FactKind::Import { .. }
        | FactKind::Reexport { .. }
        | FactKind::TypeReference
        | FactKind::ValueReference
        | FactKind::Implements
        | FactKind::Call(_)
        | FactKind::ReceiverBound { .. } => true,
        FactKind::Construct
        | FactKind::Read
        | FactKind::Store
        | FactKind::Pass
        | FactKind::Return
        | FactKind::Coerce
        | FactKind::Capture => false,
    }
}

fn dependency(from: &ModuleId, to: &ModuleId) -> DependencyId {
    DependencyId {
        from: project_map::ModuleId(from.0.clone()),
        to: project_map::ModuleId(to.0.clone()),
    }
}
