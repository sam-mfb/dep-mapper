//! Lays a draft's proposals over its base map, giving one `ProjectMap` that
//! marks each proposal, and reports what does not fit. Reading the draft and
//! its base from files is the caller's job.
//!
//! The draft's own parts become proposed additions. Each update names a base
//! part and gives only the fields it changes; the rest come from the base
//! part's review. Removing a module also marks every base dependency and
//! route that touches it for removal.
//!
//! `combine` always returns the map it could build, with every problem it
//! found, including those `ProjectMap::problems` finds in the result. A
//! caller decides whether a map with problems is fit to show.

use project_map::*;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// The combined map and what does not fit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combined {
    pub map: ProjectMap,
    pub problems: Vec<DraftProblem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftProblem {
    /// A draft's addition uses a key the base already has. The base part stays.
    Collision { part: MapPart },
    /// A draft's evidence uses a citation the base already has. The base's
    /// entry stays.
    EvidenceCollision { citation: CitationId },
    /// A draft lists a layer the base already lists.
    LayerCollision { layer: LayerName },
    /// An update names a part the base does not have.
    UpdateOfMissingPart { part: MapPart },
    /// A removal names a part the base does not have.
    RemovalOfMissingPart { part: MapPart },
    /// An update or removal names a base part that is not simply observed,
    /// such as one already proposed in the base.
    ChangeOfProposal { part: MapPart },
    /// An update of a part the base has not judged leaves out fields a
    /// judgment needs. The update is not applied.
    IncompleteUpdate {
        part: MapPart,
        missing: Vec<&'static str>,
    },
    /// A part is both updated and removed, directly or because its module is
    /// removed. The removal is kept.
    UpdatedAndRemoved { part: MapPart },
    /// Something `ProjectMap::problems` finds in the combined map.
    Map(MapProblem),
}

/// The base passed does not match what the draft names.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum BaseMismatch {
    #[error("the draft is laid over the saved map {0}, but no base map was given")]
    Missing(String),
    #[error("the draft stands alone, but a base map was given")]
    Unexpected,
}

/// Lays `draft` over `base`, which must be the map its `DraftBase::Saved`
/// names, or `None` for a draft that stands alone.
pub fn combine(draft: &Draft, base: Option<&ProjectMap>) -> Result<Combined, BaseMismatch> {
    let mut problems = vec![];
    let mut map = match (&draft.base, base) {
        (DraftBase::Saved(_), Some(base)) => base.clone(),
        (DraftBase::None { project }, None) => ProjectMap {
            project: project.clone(),
            source: None,
            layers: Layers::empty(),
            modules: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            routes: BTreeMap::new(),
            evidence: BTreeMap::new(),
            coverage_gaps: vec![],
        },
        (DraftBase::Saved(path), None) => return Err(BaseMismatch::Missing(path.clone())),
        (DraftBase::None { .. }, Some(_)) => return Err(BaseMismatch::Unexpected),
    };
    add_layers(&mut map, &draft.layers, &mut problems);
    add(
        &mut map.modules,
        &draft.modules,
        MapPart::Module,
        &mut problems,
    );
    add(
        &mut map.dependencies,
        &draft.dependencies,
        MapPart::Dependency,
        &mut problems,
    );
    add(
        &mut map.routes,
        &draft.routes,
        MapPart::Route,
        &mut problems,
    );
    for (citation, entry) in &draft.evidence {
        if map.evidence.contains_key(citation) {
            problems.push(DraftProblem::EvidenceCollision {
                citation: citation.clone(),
            });
        } else {
            map.evidence.insert(citation.clone(), entry.clone());
        }
    }
    let updated = update(&mut map, &draft.update, &mut problems);
    let removed = remove(&mut map, &draft.remove, &updated, &mut problems);
    for part in updated.intersection(&removed) {
        problems.push(DraftProblem::UpdatedAndRemoved { part: part.clone() });
    }
    problems.extend(map.problems().into_iter().map(DraftProblem::Map));
    Ok(Combined { map, problems })
}

fn add_layers(map: &mut ProjectMap, layers: &Layers, problems: &mut Vec<DraftProblem>) {
    let mut names: Vec<LayerName> = map.layers.iter().cloned().collect();
    for layer in layers.iter() {
        if names.contains(layer) {
            problems.push(DraftProblem::LayerCollision {
                layer: layer.clone(),
            });
        } else {
            names.push(layer.clone());
        }
    }
    map.layers = Layers::new(names).expect("each layer is added only once");
}

fn add<K: Ord + Clone, V: Clone>(
    parts: &mut BTreeMap<K, V>,
    additions: &BTreeMap<K, V>,
    part: impl Fn(K) -> MapPart,
    problems: &mut Vec<DraftProblem>,
) {
    for (key, value) in additions {
        if parts.contains_key(key) {
            problems.push(DraftProblem::Collision {
                part: part(key.clone()),
            });
        } else {
            parts.insert(key.clone(), value.clone());
        }
    }
}

/// Applies the updates and returns the parts updated.
fn update(
    map: &mut ProjectMap,
    updates: &Updates,
    problems: &mut Vec<DraftProblem>,
) -> BTreeSet<MapPart> {
    let mut updated = BTreeSet::new();
    for (id, change) in &updates.modules {
        let part = MapPart::Module(id.clone());
        let Some(module) = map.modules.get_mut(id) else {
            problems.push(DraftProblem::UpdateOfMissingPart { part });
            continue;
        };
        if let Some(proposed) = apply(
            &part,
            &module.status,
            &module.description,
            change,
            module_judgment,
            problems,
        ) {
            module.status = Status::ProposedUpdate { proposed };
            updated.insert(part);
        }
    }
    for (id, change) in &updates.dependencies {
        let part = MapPart::Dependency(id.clone());
        let Some(dependency) = map.dependencies.get_mut(id) else {
            problems.push(DraftProblem::UpdateOfMissingPart { part });
            continue;
        };
        if let Some(proposed) = apply(
            &part,
            &dependency.status,
            &dependency.description,
            change,
            connection_judgment,
            problems,
        ) {
            dependency.status = Status::ProposedUpdate { proposed };
            updated.insert(part);
        }
    }
    for (id, change) in &updates.routes {
        let part = MapPart::Route(id.clone());
        let Some(route) = map.routes.get_mut(id) else {
            problems.push(DraftProblem::UpdateOfMissingPart { part });
            continue;
        };
        if let Some(proposed) = apply(
            &part,
            &route.status,
            &route.description,
            change,
            connection_judgment,
            problems,
        ) {
            route.status = Status::ProposedUpdate { proposed };
            updated.insert(part);
        }
    }
    updated
}

/// The proposed review an update gives a base part, or `None` with a problem
/// when it cannot be applied. `judgment` fills the changes from the base
/// judgment, or names the fields missing when there is none.
fn apply<D, C>(
    part: &MapPart,
    status: &Status<D>,
    base: &Review<D>,
    change: &Update<C>,
    judgment: impl Fn(Option<&D>, &C) -> Result<D, Vec<&'static str>>,
    problems: &mut Vec<DraftProblem>,
) -> Option<Review<D>> {
    if !matches!(status, Status::Observed) {
        problems.push(DraftProblem::ChangeOfProposal { part: part.clone() });
        return None;
    }
    let current = match base {
        Review::Judged { judgment, .. } => Some(judgment),
        Review::Unreviewed | Review::Deferred { .. } => None,
    };
    match judgment(current, &change.changes) {
        Ok(judgment) => Some(Review::Judged {
            judgment,
            reason: change.reason.clone(),
            primary: change.primary.clone(),
            supporting: change.supporting.clone(),
        }),
        Err(missing) => {
            problems.push(DraftProblem::IncompleteUpdate {
                part: part.clone(),
                missing,
            });
            None
        }
    }
}

fn module_judgment(
    base: Option<&ModuleDescription>,
    changes: &ModuleChanges,
) -> Result<ModuleDescription, Vec<&'static str>> {
    let concern = changes
        .concern
        .clone()
        .or_else(|| base.map(|b| b.concern.clone()));
    let kind = changes
        .kind
        .clone()
        .or_else(|| base.map(|b| b.kind.clone()));
    let layer = changes
        .layer
        .clone()
        .or_else(|| base.and_then(|b| b.layer.clone()));
    match (concern, kind) {
        (Some(concern), Some(kind)) => Ok(ModuleDescription {
            concern,
            kind,
            layer,
        }),
        (concern, kind) => Err([("concern", concern.is_none()), ("kind", kind.is_none())]
            .into_iter()
            .filter_map(|(name, missing)| missing.then_some(name))
            .collect()),
    }
}

fn connection_judgment(
    base: Option<&RouteDescription>,
    changes: &ConnectionChanges,
) -> Result<RouteDescription, Vec<&'static str>> {
    let role = changes
        .role
        .clone()
        .or_else(|| base.map(|b| b.role.clone()));
    let other_roles = changes
        .other_roles
        .clone()
        .or_else(|| base.map(|b| b.other_roles.clone()))
        .unwrap_or_default();
    match role {
        Some(role) => Ok(RouteDescription { role, other_roles }),
        None => Err(vec!["role"]),
    }
}

/// Marks the removals, with the dependencies and routes of removed modules,
/// and returns every part removed. `updated` names the parts this draft
/// updated: a removal may replace those, and the caller reports it. A
/// proposal the base already holds is never replaced.
fn remove(
    map: &mut ProjectMap,
    removals: &Removals,
    updated: &BTreeSet<MapPart>,
    problems: &mut Vec<DraftProblem>,
) -> BTreeSet<MapPart> {
    let mut removed = BTreeSet::new();
    let mut modules = BTreeSet::new();
    for id in &removals.modules {
        let part = MapPart::Module(id.clone());
        match map.modules.get_mut(id) {
            Some(module) => {
                if mark_removed(&mut module.status, part, updated, &mut removed, problems) {
                    modules.insert(id.clone());
                }
            }
            None => problems.push(DraftProblem::RemovalOfMissingPart { part }),
        }
    }
    // Only parts in code go with a removed module, and only modules whose
    // removal was accepted take their parts along. An addition that rests on
    // a removed module is left for `ProjectMap::problems` to report; a part
    // the base already proposes to remove stays as it is.
    let goes_with = |status_in_code: bool, already_removed: bool, touches: bool| {
        status_in_code && !already_removed && touches
    };
    let mut dependencies: BTreeSet<DependencyId> = map
        .dependencies
        .iter()
        .filter(|(id, dependency)| {
            goes_with(
                dependency.status.in_code(),
                dependency.status.is_removal(),
                modules.contains(&id.from) || modules.contains(&id.to),
            )
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut routes: BTreeSet<RouteId> = map
        .routes
        .iter()
        .filter(|(_, route)| {
            goes_with(
                route.status.in_code(),
                route.status.is_removal(),
                route
                    .origin
                    .module()
                    .into_iter()
                    .chain(route.hops.iter().filter_map(|hop| hop.to.module()))
                    .any(|m| modules.contains(m)),
            )
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in &removals.dependencies {
        if map.dependencies.contains_key(id) {
            dependencies.insert(id.clone());
        } else {
            problems.push(DraftProblem::RemovalOfMissingPart {
                part: MapPart::Dependency(id.clone()),
            });
        }
    }
    for id in &removals.routes {
        if map.routes.contains_key(id) {
            routes.insert(id.clone());
        } else {
            problems.push(DraftProblem::RemovalOfMissingPart {
                part: MapPart::Route(id.clone()),
            });
        }
    }
    for id in dependencies {
        let dependency = map.dependencies.get_mut(&id).expect("listed from the map");
        let part = MapPart::Dependency(id);
        mark_removed(
            &mut dependency.status,
            part,
            updated,
            &mut removed,
            problems,
        );
    }
    for id in routes {
        let route = map.routes.get_mut(&id).expect("listed from the map");
        let part = MapPart::Route(id);
        mark_removed(&mut route.status, part, updated, &mut removed, problems);
    }
    removed
}

/// Marks a part for removal and says whether it was. An observed part, or
/// one this draft updated, is removed; any other proposal the base holds is
/// refused.
fn mark_removed<D>(
    status: &mut Status<D>,
    part: MapPart,
    updated: &BTreeSet<MapPart>,
    removed: &mut BTreeSet<MapPart>,
    problems: &mut Vec<DraftProblem>,
) -> bool {
    let removable = match status {
        Status::Observed => true,
        Status::ProposedUpdate { .. } => updated.contains(&part),
        Status::ProposedAddition | Status::ProposedRemoval => false,
    };
    if removable {
        *status = Status::ProposedRemoval;
        removed.insert(part);
    } else {
        problems.push(DraftProblem::ChangeOfProposal { part });
    }
    removable
}

#[cfg(test)]
mod tests;
