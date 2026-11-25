//! Proposed changes to a map, laid over a saved base map. A draft adds parts,
//! proposes new reviews for base parts, and marks base parts for removal.
//! Combining a draft with its base, and reading the base from storage, are
//! not part of this contract.

use crate::{
    CitationId, DependencyId, LayerName, Layers, MapDependency, MapEvidence, MapModule, MapRoute,
    ModuleId, ModuleKind, RouteId, UseRole,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub base: DraftBase,
    /// Layers to list after the base map's layers.
    pub layers: Layers,
    /// Proposed additions, with status `ProposedAddition`.
    pub modules: BTreeMap<ModuleId, MapModule>,
    pub dependencies: BTreeMap<DependencyId, MapDependency>,
    pub routes: BTreeMap<RouteId, MapRoute>,
    pub evidence: BTreeMap<CitationId, MapEvidence>,
    /// New reviews proposed for base parts. Empty for a draft without a base.
    pub update: Updates,
    /// Base parts proposed for removal. Empty for a draft without a base.
    pub remove: Removals,
}

/// Proposed reviews for base parts, keyed as in the base map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Updates {
    pub modules: BTreeMap<ModuleId, Update<ModuleChanges>>,
    pub dependencies: BTreeMap<DependencyId, Update<ConnectionChanges>>,
    pub routes: BTreeMap<RouteId, Update<ConnectionChanges>>,
}

/// A proposed judgment of a base part. `changes` gives only the fields that
/// change; the rest keep the base part's values. The reason and citations
/// are the update's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update<C> {
    pub changes: C,
    pub reason: String,
    pub primary: CitationId,
    pub supporting: Vec<CitationId>,
}

/// Changes to a module's judgment. `None` keeps the base value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleChanges {
    pub concern: Option<String>,
    pub kind: Option<ModuleKind>,
    pub layer: Option<LayerName>,
}

/// Changes to a dependency's or route's judgment. `None` keeps the base value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionChanges {
    pub role: Option<UseRole>,
    pub other_roles: Option<Vec<UseRole>>,
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
