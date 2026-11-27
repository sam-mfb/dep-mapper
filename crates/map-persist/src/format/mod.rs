//! The shape of a map or draft file, as serde reads and writes it. FORMAT.md
//! describes it for authors. Fields a file may leave out are `Option`s, or
//! collections that `#[serde(default)]` leaves empty; FORMAT.md lists each.
//! Values that need checks beyond their type keep their place in the text
//! (`Spanned`) so errors can point at them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use toml::Spanned;

/// The version this program writes. Format 3 adds `used_by` to surface
/// items. This program still reads formats 1 and 2.
pub const VERSION: u32 = 3;

/// Format 1 cites one fact for a route's use site, a hop and a stopped end.
/// Format 2 cites a list for each.
pub const FIRST_VERSION: u32 = 1;

/// The first version whose surface items may have `used_by`.
pub const USED_BY_VERSION: u32 = 3;

/// Read first, to choose how to read the rest.
#[derive(Deserialize)]
pub struct Header {
    pub format: Spanned<u32>,
    pub kind: Spanned<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapFile {
    pub format: u32,
    pub kind: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceDoc>,
    pub layers: Spanned<Vec<String>>,
    #[serde(default)]
    pub modules: BTreeMap<String, Spanned<ModuleDoc>>,
    #[serde(default)]
    pub dependencies: Vec<Spanned<DependencyDoc>>,
    #[serde(default)]
    pub routes: BTreeMap<String, Spanned<RouteDoc>>,
    #[serde(default)]
    pub evidence: BTreeMap<String, EvidenceDoc>,
    #[serde(default)]
    pub gaps: Vec<EvidenceDoc>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftFile {
    pub format: u32,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<Spanned<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Spanned<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Spanned<Vec<String>>>,
    #[serde(default)]
    pub modules: BTreeMap<String, Spanned<ModuleDoc>>,
    #[serde(default)]
    pub dependencies: Vec<Spanned<DependencyDoc>>,
    #[serde(default)]
    pub routes: BTreeMap<String, Spanned<RouteDoc>>,
    #[serde(default)]
    pub evidence: BTreeMap<String, EvidenceDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<Spanned<UpdateDoc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<Spanned<RemoveDoc>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDoc {
    pub graph: String,
    pub revision: String,
    pub target: String,
    pub configuration: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusDoc {
    Observed,
    ProposedAddition,
    ProposedUpdate,
    ProposedRemoval,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDoc>,
    pub name: String,
    #[serde(default)]
    pub surface: Vec<SurfaceDoc>,
    pub review: Spanned<ReviewDoc>,
    /// A proposed update's new review.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed: Option<Spanned<ReviewDoc>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceDoc {
    pub name: String,
    pub kind: ItemKindDoc,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<Spanned<LocationDoc>>,
    /// The other modules that use the item. Format 3 and later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_by: Option<Spanned<Vec<Spanned<String>>>>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKindDoc {
    Type,
    Value,
    Function,
    Method,
    Module,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKindDoc {
    Logic,
    Effect,
    Composition,
    Contract,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewStateDoc {
    Unreviewed,
    Deferred,
    Judged,
}

/// One review shape for modules, dependencies and routes. Which fields are
/// allowed depends on `state` and on what is reviewed; decoding checks them.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDoc {
    pub state: ReviewStateDoc,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Modules only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concern: Option<String>,
    /// Modules only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ModuleKindDoc>,
    /// Modules only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Dependencies and routes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Spanned<String>>,
    /// Dependencies and routes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_roles: Option<Vec<Spanned<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supporting: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyDoc {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDoc>,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub review: Spanned<ReviewDoc>,
    /// A proposed update's new review.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed: Option<Spanned<ReviewDoc>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDoc>,
    /// Format 1 only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_site: Option<Spanned<String>>,
    /// Format 2 only. Left out means none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_sites: Option<Spanned<Vec<String>>>,
    pub origin: Spanned<PointDoc>,
    #[serde(default)]
    pub hops: Vec<HopDoc>,
    pub end: Spanned<EndDoc>,
    pub review: Spanned<ReviewDoc>,
    /// A proposed update's new review.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed: Option<Spanned<ReviewDoc>>,
}

/// A point in the project (`module`, optional `item`, and for a hop `link`),
/// or outside it (`external` or `unresolved`). Decoding checks the mix.
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PointDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<LinkDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkDoc {
    ReceiverUsesSender,
    SenderUsesReceiver,
    SameModule,
    FromOutside,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HopDoc {
    pub to: Spanned<PointDoc>,
    pub evidence: Spanned<Citations>,
    pub support: Spanned<SupportDoc>,
    /// Possible hops only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Spanned<String>>,
}

/// Citation IDs: one in format 1, a list in format 2 and later. Decoding checks the
/// form against the file's version.
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum Citations {
    One(String),
    List(Vec<String>),
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportDoc {
    Established,
    Possible,
    Proposed,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndStateDoc {
    Reached,
    Stopped,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EndDoc {
    pub state: EndStateDoc,
    /// Stopped routes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<Citations>,
    /// Stopped routes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKindDoc {
    Source,
    Proposal,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceDoc {
    pub kind: EvidenceKindDoc,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Spanned<LocationDoc>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocationDoc {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveDoc {
    #[serde(default)]
    pub modules: Vec<Spanned<String>>,
    #[serde(default)]
    pub dependencies: Vec<Spanned<DependencyKeyDoc>>,
    #[serde(default)]
    pub routes: Vec<Spanned<String>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyKeyDoc {
    pub from: String,
    pub to: String,
}

/// A draft's proposed reviews for base parts.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateDoc {
    #[serde(default)]
    pub modules: BTreeMap<String, Spanned<ChangeDoc>>,
    #[serde(default)]
    pub dependencies: Vec<Spanned<DependencyChangeDoc>>,
    #[serde(default)]
    pub routes: BTreeMap<String, Spanned<ChangeDoc>>,
}

/// The fields an update changes, with its own reason and citations. Which
/// fields are allowed depends on what is updated; decoding checks them.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ModuleKindDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Spanned<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_roles: Option<Vec<Spanned<String>>>,
    pub reason: String,
    pub primary: String,
    #[serde(default)]
    pub supporting: Vec<String>,
}

/// A dependency's update: the dependency's key and a `ChangeDoc`'s fields
/// for dependencies.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyChangeDoc {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Spanned<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_roles: Option<Vec<Spanned<String>>>,
    pub reason: String,
    pub primary: String,
    #[serde(default)]
    pub supporting: Vec<String>,
}
