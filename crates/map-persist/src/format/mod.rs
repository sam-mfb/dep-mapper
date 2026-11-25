//! The shape of a map or draft file, as serde reads and writes it. FORMAT.md
//! describes it for authors. Fields a file may leave out are `Option`s, or
//! collections that `#[serde(default)]` leaves empty; FORMAT.md lists each.
//! Values that need checks beyond their type keep their place in the text
//! (`Spanned`) so errors can point at them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use toml::Spanned;

pub const VERSION: u32 = 1;

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
    Proposed,
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
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_site: Option<String>,
    pub origin: Spanned<PointDoc>,
    #[serde(default)]
    pub hops: Vec<HopDoc>,
    pub end: Spanned<EndDoc>,
    pub review: Spanned<ReviewDoc>,
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
    pub evidence: String,
    pub support: Spanned<SupportDoc>,
    /// Possible hops only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Spanned<String>>,
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
    pub at: Option<String>,
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
