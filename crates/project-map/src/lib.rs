//! A self-contained, reviewed project map. Consumers can inspect the map,
//! follow a particular route and read its evidence using this contract alone.
//! The builder resolves graph and evaluation references into this view.
//! Layout, file encoding and the rules for matching judgments belong to consumers.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMap {
    /// Identifies the graph snapshot from which all subjects and citations came.
    pub graph_id: String,
    pub project: String,
    pub source_revision: String,
    pub target: String,
    pub configuration: String,
    pub layers: Vec<String>,
    /// Keys retain the identity of modules and routes in the source snapshot.
    pub modules: BTreeMap<String, MapModule>,
    pub routes: BTreeMap<String, MapRoute>,
    pub evidence: BTreeMap<CitationId, MapEvidence>,
    pub coverage_gaps: Vec<MapEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapModule {
    pub name: String,
    pub surface: Vec<SurfaceItem>,
    pub review: Review<ModuleDescription>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceItem {
    pub name: String,
    pub kind: String,
    pub signature: Option<String>,
    pub declaration: Option<Location>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDescription {
    pub concern: String,
    pub kind: ModuleKind,
    pub layer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapRoute {
    /// Identifies the same use across alternate or incomplete routes.
    pub use_site: Option<CitationId>,
    pub origin: MapEndpoint,
    /// In supply order, starting at origin. Parallel routes retain separate IDs.
    pub hops: Vec<MapHop>,
    pub end: MapRouteEnd,
    /// The architectural uses assigned to this particular path.
    pub review: Review<RouteDescription>,
}

/// Architectural roles attached to this map route by the builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDescription {
    pub role: UseRole,
    pub other_roles: Vec<UseRole>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseRole {
    DirectUse,
    Implements,
    Wires,
    SuppliesInput,
    FullInjection,
    AdaptedInjection,
    Incidental,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Logic,
    Effect,
    Composition,
    Contract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapEndpoint {
    /// Module key and optional operation, value or field shown on drill-down.
    Project {
        module: String,
        item: Option<String>,
    },
    External(String),
    Unresolved(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapHop {
    pub to: MapEndpoint,
    pub evidence: CitationId,
    pub support: HopSupport,
    pub role: Option<UseRole>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopSupport {
    Established,
    Possible { condition: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapRouteEnd {
    ReachedUse,
    Stopped { at: CitationId, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Review<T> {
    Unreviewed,
    Deferred {
        reason: String,
    },
    Judged {
        description: T,
        reason: String,
        primary: CitationId,
        supporting: Vec<CitationId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CitationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEvidence {
    pub build_unit: Option<String>,
    pub summary: String,
    /// Build facts and missing coverage may have no source text.
    pub location: Option<Location>,
}

/// A project-relative source span in the map's source revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    /// Half-open UTF-8 byte offsets.
    pub bytes: std::ops::Range<usize>,
}
