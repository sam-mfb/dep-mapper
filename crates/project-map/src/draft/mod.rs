//! Proposed changes to a map, laid over a saved base map. A draft adds parts
//! and marks base parts for removal. Combining a draft with its base, and
//! reading the base from storage, are not part of this contract.

use crate::{
    CitationId, DependencyId, Layers, MapDependency, MapEvidence, MapModule, MapRoute, ModuleId,
    RouteId,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub base: DraftBase,
    /// Layers to list after the base map's layers.
    pub layers: Layers,
    pub modules: BTreeMap<ModuleId, MapModule>,
    pub dependencies: BTreeMap<DependencyId, MapDependency>,
    pub routes: BTreeMap<RouteId, MapRoute>,
    pub evidence: BTreeMap<CitationId, MapEvidence>,
    /// Base parts proposed for removal. Empty for a draft without a base.
    pub remove: Removals,
}

/// What a draft is laid over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftBase {
    /// A saved map, as the draft names it. The project's name comes from it.
    Saved(String),
    /// Nothing: the draft is a map made only of proposals.
    None { project: String },
}

/// Names base parts by their keys in the base map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removals {
    pub modules: BTreeSet<ModuleId>,
    pub dependencies: BTreeSet<DependencyId>,
    pub routes: BTreeSet<RouteId>,
}
