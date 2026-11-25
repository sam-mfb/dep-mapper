//! Checking that the parts of a map fit together. `map-builder` guarantees
//! this for the maps it builds; maps written by hand need the check.
//!
//! "In code" means observed, or observed and proposed for removal. A part in
//! code cannot rest on a proposed part or cite a proposal note. A proposed
//! part can rest on parts in code, but not on one proposed for removal.

use crate::{
    CitationId, DependencyId, EvidenceKind, HopSupport, LayerName, MapRoute, MapRouteEnd, ModuleId,
    ProjectMap, Review, RouteId, Status,
};
use std::collections::BTreeSet;

/// Names a module, dependency or route of a map.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MapPart {
    Module(ModuleId),
    Dependency(DependencyId),
    Route(RouteId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapProblem {
    /// The part cites an entry that is not in the map's evidence.
    MissingEvidence { part: MapPart, citation: CitationId },
    /// A dependency or route names a module that is not on the map.
    MissingModule { part: MapPart, module: ModuleId },
    /// A route hop travels along a dependency that is not on the map.
    MissingDependency {
        route: RouteId,
        hop: usize,
        dependency: DependencyId,
    },
    /// A part in code rests on a proposed part.
    ObservedRestsOnProposed { part: MapPart, proposed: MapPart },
    /// A part in code cites a proposal note.
    ObservedCitesProposal { part: MapPart, citation: CitationId },
    /// A proposed route has a hop that is not proposed, or a route in code
    /// has a proposed hop.
    HopStatusMismatch { route: RouteId, hop: usize },
    /// A proposed part rests on a part proposed for removal.
    ProposedRestsOnRemoval { part: MapPart, removed: MapPart },
    /// A module's layer is not one of the map's layers.
    UnlistedLayer { module: ModuleId, layer: LayerName },
}

impl ProjectMap {
    /// Every problem found, in the order of modules, dependencies, then routes.
    /// An empty list means the map is consistent.
    pub fn problems(&self) -> Vec<MapProblem> {
        let mut problems = vec![];
        for (id, module) in &self.modules {
            let part = MapPart::Module(id.clone());
            self.check_citations(
                &part,
                &module.status,
                review_citations(&module.description),
                &mut problems,
            );
            if let Review::Judged { judgment, .. } = &module.description
                && let Some(layer) = &judgment.layer
                && !self.layers.contains(layer)
            {
                problems.push(MapProblem::UnlistedLayer {
                    module: id.clone(),
                    layer: layer.clone(),
                });
            }
        }
        for (id, dependency) in &self.dependencies {
            let part = MapPart::Dependency(id.clone());
            for module in BTreeSet::from([&id.from, &id.to]) {
                self.check_module(&part, &dependency.status, module, &mut problems);
            }
            let cited = dependency
                .evidence
                .iter()
                .chain(review_citations(&dependency.description))
                .collect();
            self.check_citations(&part, &dependency.status, cited, &mut problems);
        }
        for (id, route) in &self.routes {
            self.check_route(id, route, &mut problems);
        }
        problems
    }

    fn check_route(&self, id: &RouteId, route: &MapRoute, problems: &mut Vec<MapProblem>) {
        let part = MapPart::Route(id.clone());
        let modules: BTreeSet<&ModuleId> = route
            .origin
            .module()
            .into_iter()
            .chain(route.hops.iter().filter_map(|hop| hop.to.module()))
            .collect();
        for module in modules {
            self.check_module(&part, &route.status, module, problems);
        }
        let mut dependencies = BTreeSet::new();
        for (index, hop) in route.hops.iter().enumerate() {
            let proposed_hop = hop.support == HopSupport::Proposed;
            if proposed_hop == route.status.in_code() {
                problems.push(MapProblem::HopStatusMismatch {
                    route: id.clone(),
                    hop: index,
                });
            }
            let Some(dependency) = route.hop_dependency(index) else {
                continue;
            };
            if self.dependencies.contains_key(&dependency) {
                dependencies.insert(dependency);
            } else {
                problems.push(MapProblem::MissingDependency {
                    route: id.clone(),
                    hop: index,
                    dependency,
                });
            }
        }
        for dependency in dependencies {
            let status = &self.dependencies[&dependency].status;
            check_rests_on(
                &part,
                &route.status,
                MapPart::Dependency(dependency),
                status,
                problems,
            );
        }
        let cited = route
            .use_site
            .iter()
            .chain(route.hops.iter().map(|hop| &hop.evidence))
            .chain(match &route.end {
                MapRouteEnd::Stopped { at, .. } => Some(at),
                MapRouteEnd::ReachedUse => None,
            })
            .chain(review_citations(&route.description))
            .collect();
        self.check_citations(&part, &route.status, cited, problems);
    }

    fn check_module(
        &self,
        part: &MapPart,
        status: &Status,
        module: &ModuleId,
        problems: &mut Vec<MapProblem>,
    ) {
        match self.modules.get(module) {
            Some(found) => check_rests_on(
                part,
                status,
                MapPart::Module(module.clone()),
                &found.status,
                problems,
            ),
            None => problems.push(MapProblem::MissingModule {
                part: part.clone(),
                module: module.clone(),
            }),
        }
    }

    fn check_citations(
        &self,
        part: &MapPart,
        status: &Status,
        cited: Vec<&CitationId>,
        problems: &mut Vec<MapProblem>,
    ) {
        for citation in cited.into_iter().collect::<BTreeSet<_>>() {
            match self.evidence.get(citation) {
                None => problems.push(MapProblem::MissingEvidence {
                    part: part.clone(),
                    citation: citation.clone(),
                }),
                Some(evidence) if status.in_code() && evidence.kind == EvidenceKind::Proposal => {
                    problems.push(MapProblem::ObservedCitesProposal {
                        part: part.clone(),
                        citation: citation.clone(),
                    })
                }
                Some(_) => {}
            }
        }
    }
}

fn check_rests_on(
    part: &MapPart,
    status: &Status,
    other: MapPart,
    other_status: &Status,
    problems: &mut Vec<MapProblem>,
) {
    match (status, other_status) {
        (Status::Observed | Status::ProposedRemoval, Status::Proposed) => {
            problems.push(MapProblem::ObservedRestsOnProposed {
                part: part.clone(),
                proposed: other,
            })
        }
        (Status::Proposed, Status::ProposedRemoval) => {
            problems.push(MapProblem::ProposedRestsOnRemoval {
                part: part.clone(),
                removed: other,
            })
        }
        _ => {}
    }
}

fn review_citations<T>(review: &Review<T>) -> Vec<&CitationId> {
    match review {
        Review::Judged {
            primary,
            supporting,
            ..
        } => std::iter::once(primary).chain(supporting).collect(),
        Review::Unreviewed | Review::Deferred { .. } => vec![],
    }
}

#[cfg(test)]
mod tests;
