//! The source evidence for one project build. Identifiers join records within
//! this snapshot. The grapher assigns them and retains separate route IDs for
//! different wiring sites, even when their supplier and consumer coincide.
//! Route hops follow the capability or value from its origin toward a consumer.

mod graph_ref;

pub use graph_ref::{Cited, GraphRef};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidenceId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectGraph {
    /// Identifies this evidence snapshot, including its source and build selection.
    pub id: GraphId,
    pub build: Build,
    pub modules: BTreeMap<ModuleId, Module>,
    pub units: BTreeMap<UnitId, BuildUnit>,
    pub items: BTreeMap<ItemId, Item>,
    pub evidence: BTreeMap<EvidenceId, Evidence>,
    pub routes: BTreeMap<RouteId, Route>,
    /// The ways modules are connected, each of which can be judged.
    pub connections: BTreeMap<ConnectionId, Connection>,
    /// Missing coverage can apply to a whole unit before any route is discovered.
    pub gaps: Vec<CoverageGap>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub project: String,
    /// Identifies the source snapshot, including local changes when applicable.
    pub source_revision: String,
    pub target: String,
    pub configuration: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub root: String,
}

/// A separately analyzed unit of a package, such as a test target with features.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildUnit {
    pub module: ModuleId,
    pub target: String,
    pub settings: BTreeMap<String, String>,
}

/// A project-relative path and a half-open range of UTF-8 byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub file: String,
    pub bytes: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub module: ModuleId,
    pub name: String,
    pub kind: ItemKind,
    pub visibility: Visibility,
    /// Language-level declaration or signature when available.
    pub signature: Option<String>,
    pub declaration: Option<SourceSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Type,
    Value,
    Function,
    Method,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visibility {
    Exported,
    Internal,
    /// The analysis did not establish effective visibility.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Module(ModuleId),
    Item(ItemId),
    /// A named field, argument, capture or local within an item.
    Place {
        owner: ItemId,
        projection: String,
    },
    /// The visible side of a handoff beyond the project.
    External(String),
    Unresolved(String),
}

/// A fact about two endpoints in one build unit. For a call, the subject is
/// the caller and the object is the called operation. For a value transfer
/// they are the source and destination. Several facts may cite one span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub unit: UnitId,
    pub subject: Endpoint,
    pub kind: Fact,
    pub object: Endpoint,
    /// Build inventories and compiler-generated facts can lack source text.
    pub site: Option<SourceSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    DeclaredDependency,
    Import {
        local_name: String,
        usage: ImportUse,
    },
    Reexport {
        exported_name: String,
    },
    TypeReference,
    ValueReference,
    Implements,
    Call(Dispatch),
    /// The object identifies a receiver item or its package at this call.
    /// A receiver bound can be known even when its handoff route is incomplete.
    ReceiverBound {
        call: EvidenceId,
        basis: ReceiverBasis,
    },
    Construct,
    Store,
    Read,
    Pass,
    Return,
    Coerce,
    Capture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportUse {
    Unused,
    TypeOnly,
    Value,
    Undetermined,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverBasis {
    SelectedByCompiler,
    UniqueInClosedBuild,
    /// The build admits this provider; reaching the call requires a witness.
    BuildCandidate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Direct,
    SelectedImplementation,
    Contract,
    Indirect,
    Foreign,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// The use being explained, when known. Alternative and stopped traces
    /// can belong to the same call site; a supply with no known use has none.
    pub use_site: Option<EvidenceId>,
    /// The connection this route traces, when it joins two project modules.
    pub connection: Option<ConnectionId>,
    pub origin: Endpoint,
    pub hops: Vec<Hop>,
    pub end: RouteEnd,
}

/// The previous hop (or the route origin) supplies the starting endpoint.
/// The cited fact retains the original source orientation and location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub to: Endpoint,
    pub evidence: EvidenceId,
    pub support: Support,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Support {
    /// Established for this build. Execution at runtime is a separate question.
    Established,
    Possible {
        condition: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteEnd {
    ReachedUse,
    /// The last explained endpoint can be the origin if no hop was recovered.
    Stopped {
        at: EvidenceId,
        reason: String,
    },
}

/// One way a module uses another. The use can be in `user`'s own code or
/// arrive through other modules, as when a capability `used` supplies is
/// handed to `user`. Two modules can be connected in several ways; how the
/// grapher groups facts into connections is its own choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub user: ModuleId,
    pub used: ModuleId,
    /// The facts that establish the connection.
    pub evidence: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageGap {
    pub unit: Option<UnitId>,
    pub site: Option<SourceSpan>,
    pub reason: String,
}
