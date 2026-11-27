//! Checking that the parts of a map fit together. `map-builder` guarantees
//! this for the maps it builds; maps written by hand need the check.
//!
//! A part is in code unless it is a proposed addition. A part in code cannot
//! rest on an addition, and neither its evidence nor its review as found in
//! code can cite a proposal note. A proposed review (an addition's, or the
//! one an update carries) can cite notes. A proposed addition or update can
//! rest on parts in code, but not on one proposed for removal.

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
    /// A dependency, a route or a module's surface item names a module that
    /// is not on the map. For a surface item, `part` is the item's module.
    MissingModule { part: MapPart, module: ModuleId },
    /// A route hop travels along a dependency that is not on the map.
    MissingDependency {
        route: RouteId,
        hop: usize,
        dependency: DependencyId,
    },
    /// A part in code rests on a proposed addition.
    ObservedRestsOnAddition { part: MapPart, addition: MapPart },
    /// A part's evidence, or its review as found in code, cites a proposal note.
    ObservedCitesProposal { part: MapPart, citation: CitationId },
    /// A proposed route has a hop that is not proposed, or a route in code
    /// has a proposed hop.
    HopStatusMismatch { route: RouteId, hop: usize },
    /// A proposed addition or update rests on a part proposed for removal.
    ProposedRestsOnRemoval { part: MapPart, removed: MapPart },
    /// A proposed update reaches the same decision as the review in code.
    UpdateChangesNothing { part: MapPart },
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
            self.check_reviews(
                &part,
                &module.status,
                &module.description,
                vec![],
                &mut problems,
            );
            let proposed = match &module.status {
                Status::ProposedUpdate { proposed } => Some(proposed),
                _ => None,
            };
            let layers: BTreeSet<&LayerName> = std::iter::once(&module.description)
                .chain(proposed)
                .filter_map(|review| match review {
                    Review::Judged { judgment, .. } => judgment.layer.as_ref(),
                    Review::Unreviewed | Review::Deferred { .. } => None,
                })
                .collect();
            for layer in layers {
                if !self.layers.contains(layer) {
                    problems.push(MapProblem::UnlistedLayer {
                        module: id.clone(),
                        layer: layer.clone(),
                    });
                }
            }
            let users: BTreeSet<&ModuleId> = module
                .surface
                .iter()
                .flat_map(|item| &item.used_by)
                .collect();
            for user in users {
                if !self.modules.contains_key(user) {
                    problems.push(MapProblem::MissingModule {
                        part: part.clone(),
                        module: user.clone(),
                    });
                }
            }
        }
        for (id, dependency) in &self.dependencies {
            let part = MapPart::Dependency(id.clone());
            for module in BTreeSet::from([&id.from, &id.to]) {
                self.check_module(&part, class(&dependency.status), module, &mut problems);
            }
            let evidence = dependency.evidence.iter().collect();
            self.check_reviews(
                &part,
                &dependency.status,
                &dependency.description,
                evidence,
                &mut problems,
            );
        }
        for (id, route) in &self.routes {
            self.check_route(id, route, &mut problems);
        }
        problems
    }

    fn check_route(&self, id: &RouteId, route: &MapRoute, problems: &mut Vec<MapProblem>) {
        let part = MapPart::Route(id.clone());
        let status = class(&route.status);
        let modules: BTreeSet<&ModuleId> = route
            .origin
            .module()
            .into_iter()
            .chain(route.hops.iter().filter_map(|hop| hop.to.module()))
            .collect();
        for module in modules {
            self.check_module(&part, status, module, problems);
        }
        let mut dependencies = BTreeSet::new();
        for (index, hop) in route.hops.iter().enumerate() {
            let proposed_hop = hop.support == HopSupport::Proposed;
            if proposed_hop != route.status.is_addition() {
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
            let other = class(&self.dependencies[&dependency].status);
            check_rests_on(
                &part,
                status,
                MapPart::Dependency(dependency),
                other,
                problems,
            );
        }
        let stopped_at = match &route.end {
            MapRouteEnd::Stopped { at, .. } => at.as_slice(),
            MapRouteEnd::ReachedUse => &[],
        };
        let evidence = route
            .use_sites
            .iter()
            .chain(route.hops.iter().flat_map(|hop| &hop.evidence))
            .chain(stopped_at)
            .collect();
        self.check_reviews(&part, &route.status, &route.description, evidence, problems);
    }

    /// Checks what a part cites, and that an update changes its judgment.
    /// `evidence` is what the part cites outside its reviews.
    fn check_reviews<D: PartialEq>(
        &self,
        part: &MapPart,
        status: &Status<D>,
        description: &Review<D>,
        evidence: Vec<&CitationId>,
        problems: &mut Vec<MapProblem>,
    ) {
        let mut in_code = vec![];
        let mut proposed = vec![];
        if status.in_code() {
            in_code.extend(evidence);
            in_code.extend(review_citations(description));
        } else {
            proposed.extend(evidence);
            proposed.extend(review_citations(description));
        }
        if let Status::ProposedUpdate { proposed: update } = status {
            proposed.extend(review_citations(update));
            if same_decision(update, description) {
                problems.push(MapProblem::UpdateChangesNothing { part: part.clone() });
            }
        }
        self.check_citations(part, true, in_code, problems);
        self.check_citations(part, false, proposed, problems);
    }

    fn check_module(
        &self,
        part: &MapPart,
        status: Class,
        module: &ModuleId,
        problems: &mut Vec<MapProblem>,
    ) {
        match self.modules.get(module) {
            Some(found) => check_rests_on(
                part,
                status,
                MapPart::Module(module.clone()),
                class(&found.status),
                problems,
            ),
            None => problems.push(MapProblem::MissingModule {
                part: part.clone(),
                module: module.clone(),
            }),
        }
    }

    /// `in_code`: whether the citations must not name proposal notes.
    fn check_citations(
        &self,
        part: &MapPart,
        in_code: bool,
        cited: Vec<&CitationId>,
        problems: &mut Vec<MapProblem>,
    ) {
        for citation in cited.into_iter().collect::<BTreeSet<_>>() {
            match self.evidence.get(citation) {
                None => problems.push(MapProblem::MissingEvidence {
                    part: part.clone(),
                    citation: citation.clone(),
                }),
                Some(evidence) if in_code && evidence.kind == EvidenceKind::Proposal => problems
                    .push(MapProblem::ObservedCitesProposal {
                        part: part.clone(),
                        citation: citation.clone(),
                    }),
                Some(_) => {}
            }
        }
    }
}

/// A status without the review an update carries.
#[derive(Clone, Copy)]
enum Class {
    Observed,
    Addition,
    Update,
    Removal,
}

fn class<D>(status: &Status<D>) -> Class {
    match status {
        Status::Observed => Class::Observed,
        Status::ProposedAddition => Class::Addition,
        Status::ProposedUpdate { .. } => Class::Update,
        Status::ProposedRemoval => Class::Removal,
    }
}

fn check_rests_on(
    part: &MapPart,
    status: Class,
    other: MapPart,
    other_status: Class,
    problems: &mut Vec<MapProblem>,
) {
    match (status, other_status) {
        (Class::Observed | Class::Update | Class::Removal, Class::Addition) => {
            problems.push(MapProblem::ObservedRestsOnAddition {
                part: part.clone(),
                addition: other,
            })
        }
        (Class::Addition | Class::Update, Class::Removal) => {
            problems.push(MapProblem::ProposedRestsOnRemoval {
                part: part.clone(),
                removed: other,
            })
        }
        _ => {}
    }
}

/// Whether two reviews reach the same decision: equal judgments, or the same
/// state without a judgment.
fn same_decision<D: PartialEq>(a: &Review<D>, b: &Review<D>) -> bool {
    match (a, b) {
        (Review::Judged { judgment: a, .. }, Review::Judged { judgment: b, .. }) => a == b,
        (Review::Unreviewed, Review::Unreviewed) => true,
        (Review::Deferred { .. }, Review::Deferred { .. }) => true,
        _ => false,
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
