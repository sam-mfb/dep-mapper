//! A self-contained, reviewed project map. Consumers can inspect the map,
//! follow a particular route and read its evidence using this contract alone.
//! The builder resolves graph and evaluation references into this view.
//! Layout, file encoding and the rules for matching judgments belong to consumers.
//!
//! A map can also hold proposals. Each module, dependency and route has a
//! `Status`: observed in code, or a proposed addition, update or removal.
//! A map made only of proposals has no `source`.
//! `ProjectMap::problems` checks that the parts of a map fit together.
//! A `Draft` holds proposed changes to a saved map.

mod draft;
mod layers;
mod problems;
mod route;

pub use draft::{ConnectionChanges, Draft, DraftBase, ModuleChanges, Removals, Update, Updates};
pub use layers::{DuplicateLayer, Layers};
pub use problems::{MapPart, MapProblem};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMap {
    /// The project's name, for people to read.
    pub project: String,
    /// The analyzed build the observed parts came from. `None` when the map
    /// holds only proposals.
    pub source: Option<MapSource>,
    pub layers: Layers,
    /// Keys retain the identity of modules and routes in the source snapshot.
    pub modules: BTreeMap<ModuleId, MapModule>,
    /// Module-to-module dependencies for the overview, keyed by the modules they join.
    pub dependencies: BTreeMap<DependencyId, MapDependency>,
    pub routes: BTreeMap<RouteId, MapRoute>,
    pub evidence: BTreeMap<CitationId, MapEvidence>,
    pub coverage_gaps: Vec<MapEvidence>,
}

/// The analyzed build behind a map's observed parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSource {
    /// Identifies the graph snapshot from which the observed subjects and
    /// source citations came.
    pub graph_id: GraphId,
    pub source_revision: SourceRevision,
    pub target: String,
    pub configuration: String,
}

/// Whether a part of the map exists in code, and what is proposed for it.
/// `D` is the part's description: `ModuleDescription` for a module,
/// `RouteDescription` for a dependency or route. For a part in code, the
/// part's own `description` is the review of the part as it is in code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status<D> {
    Observed,
    /// Not in code: proposed as a new part. Its `description` is proposed too.
    ProposedAddition,
    /// In code, with `proposed` as its new review.
    ProposedUpdate {
        proposed: Review<D>,
    },
    /// In code and proposed for removal.
    ProposedRemoval,
}

impl<D> Status<D> {
    /// Whether the part exists in code: every status but an addition.
    pub fn in_code(&self) -> bool {
        !self.is_addition()
    }

    pub fn is_addition(&self) -> bool {
        matches!(self, Status::ProposedAddition)
    }

    pub fn is_removal(&self) -> bool {
        matches!(self, Status::ProposedRemoval)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapModule {
    pub status: Status<ModuleDescription>,
    pub name: String,
    pub surface: Vec<SurfaceItem>,
    pub description: Review<ModuleDescription>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceItem {
    pub name: String,
    pub kind: ItemKind,
    pub signature: Option<String>,
    pub declaration: Option<Location>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDescription {
    pub concern: String,
    pub kind: ModuleKind,
    pub layer: Option<LayerName>,
}

/// One module's use of another, summarized across all the ways it uses it.
/// A route explains a particular path; a dependency is the arrow on the overview.
/// Its modules are given by its key in `ProjectMap::dependencies`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapDependency {
    pub status: Status<RouteDescription>,
    /// Facts behind the dependency, such as imports, references and calls.
    pub evidence: Vec<CitationId>,
    /// The architectural uses assigned to the dependency as a whole.
    pub description: Review<RouteDescription>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapRoute {
    /// A proposed addition has only proposed hops; a route in code has none.
    pub status: Status<RouteDescription>,
    /// The uses this path explains, one for each site whose value follows
    /// it. Identifies the same use across alternate or incomplete routes.
    pub use_sites: Vec<CitationId>,
    pub origin: RoutePoint,
    /// In supply order, starting at origin. Routes that follow different paths stay separate.
    pub hops: Vec<MapHop>,
    pub end: MapRouteEnd,
    /// The architectural uses assigned to this particular path.
    pub description: Review<RouteDescription>,
}

/// Architectural roles attached to a route or dependency by the builder.
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

/// What sort of code item a surface item is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Type,
    Value,
    Function,
    Method,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Logic,
    Effect,
    Composition,
    Contract,
}

/// Where a route starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutePoint {
    Project(ProjectPoint),
    Outside(OutsidePoint),
}

/// A module and optional operation, value or field shown on drill-down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPoint {
    pub module: ModuleId,
    pub item: Option<String>,
}

/// A point outside the project's modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutsidePoint {
    External(String),
    Unresolved(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapHop {
    pub to: HopTarget,
    /// At least one: every fact behind this step, one for each use site
    /// that takes it, or the proposal note of a proposed hop.
    pub evidence: Vec<CitationId>,
    pub support: HopSupport,
    pub role: Option<UseRole>,
}

/// Where a hop arrives. A hop into a project module says how it got there;
/// see `MapRoute::hop_dependency`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopTarget {
    Project { point: ProjectPoint, link: HopLink },
    Outside(OutsidePoint),
}

/// How a hop into a project module relates to the point it left. Supply can
/// travel either way along a dependency: a program obtains a value from a
/// module it uses, then passes the value into another module it uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopLink {
    /// The module the value moves into uses the module it comes from.
    ReceiverUsesSender,
    /// The module the value comes from uses the module it moves into.
    SenderUsesReceiver,
    /// The value moves between items of one module.
    SameModule,
    /// The value comes from outside the project or from an unresolved point.
    FromOutside,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HopSupport {
    Established,
    Possible {
        condition: Option<String>,
    },
    /// Part of a proposed route; nothing in code supports it yet.
    Proposed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapRouteEnd {
    ReachedUse,
    /// `at` cites, for each use that stopped this way, where it stopped.
    Stopped {
        at: Vec<CitationId>,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Review<T> {
    Unreviewed,
    Deferred {
        reason: String,
    },
    Judged {
        judgment: T,
        reason: String,
        primary: CitationId,
        supporting: Vec<CitationId>,
    },
}

// Identifiers. Each names one thing, so two equal values mean the same thing.
// Text meant only for people stays a plain `String`.

/// Identifies the graph snapshot a map was built from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphId(pub String);

/// Identifies the revision of the source that was analyzed, such as a commit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceRevision(pub String);

/// Names one of the map's layers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayerName(pub String);

/// Identifies a module, as keyed in `ProjectMap::modules`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleId(pub String);

/// Identifies a route, as keyed in `ProjectMap::routes`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteId(pub String);

/// Identifies a build unit, such as a library or binary target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitId(pub String);

/// A file path relative to the project root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourcePath(pub String);

/// Identifies a dependency: `from` uses `to`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyId {
    pub from: ModuleId,
    pub to: ModuleId,
}

/// Identifies an entry in `ProjectMap::evidence`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CitationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEvidence {
    pub kind: EvidenceKind,
    pub build_unit: Option<UnitId>,
    pub summary: String,
    /// Build facts and missing coverage may have no source text.
    pub location: Option<Location>,
}

/// Where a piece of evidence comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Found by analyzing the source.
    Source,
    /// An author's reasoning for a proposal. Only proposed parts may cite it.
    Proposal,
}

/// A project-relative source span in the map's source revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: SourcePath,
    /// Half-open UTF-8 byte offsets.
    pub bytes: std::ops::Range<usize>,
}

/// sharsheret's own architecture as a sample map for consumers' previews and tests.
#[cfg(any(test, feature = "example"))]
pub mod example;
