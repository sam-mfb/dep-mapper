//! The page's own representation of a map. The browser script reads this
//! shape; nothing outside map-web depends on it. Identifiers become plain
//! JSON strings here.

use project_map::{
    CitationId, HopSupport, ItemKind, Location, MapEvidence, MapRouteEnd, ModuleId, ModuleKind,
    OutsidePoint, ProjectMap, ProjectPoint, Review, RouteId, RoutePoint, UseRole,
};
use serde::Serialize;
use std::collections::BTreeMap;

/// The band for modules whose review assigns no layer listed by the map.
pub const UNASSIGNED_LAYER: &str = "Unassigned";

#[derive(Debug, Serialize)]
pub struct View {
    pub meta: Meta,
    pub layers: Vec<String>,
    pub crates: Vec<Crate>,
    pub edges: Vec<Edge>,
    /// Contract modules keyed to the implementations drawn beneath them.
    pub groups: BTreeMap<String, Vec<String>>,
    pub routes: Vec<Route>,
    pub evidence: BTreeMap<String, Evidence>,
    pub gaps: Vec<Evidence>,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub project: String,
    /// Absent when the map holds only proposals.
    pub source: Option<SourceMeta>,
}

#[derive(Debug, Serialize)]
pub struct SourceMeta {
    pub graph_id: String,
    pub revision: String,
    pub target: String,
    pub configuration: String,
}

#[derive(Debug, Serialize)]
pub struct Crate {
    pub name: String,
    /// A ModuleKind slug, or "unreviewed" when the module has no judgment.
    pub kind: String,
    pub layer: String,
    pub concern: String,
    /// Draws the program's show and hide control on its box.
    pub program: bool,
    pub surface: Vec<SurfaceItem>,
    pub review: ReviewView,
}

#[derive(Debug, Serialize)]
pub struct SurfaceItem {
    pub name: String,
    pub kind: &'static str,
    pub signature: Option<String>,
    pub location: Option<LocationView>,
}

#[derive(Debug, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Role slugs, primary first. An edge without a judgment has "unreviewed".
    pub roles: Vec<String>,
    /// The names of any `Other` roles, in order.
    pub other_roles: Vec<String>,
    pub detail: String,
    pub evidence: Vec<String>,
    pub review: ReviewView,
}

#[derive(Debug, Serialize)]
pub struct Route {
    pub id: String,
    /// Project modules along the route, origin first, without repeats in a row.
    pub modules: Vec<String>,
    pub origin: PointView,
    /// What travels along the route: the origin's item, or its name.
    pub carry: String,
    /// The role slug of the first hop, or of the route when the hop has none.
    pub first_role: String,
    /// Role slugs judged for the route, primary first; empty when not judged.
    pub roles: Vec<String>,
    /// The names of any `Other` roles, in order.
    pub other_roles: Vec<String>,
    /// Drawn on the map as an injection; otherwise described in the panel only.
    pub injection: bool,
    pub use_site: Option<String>,
    pub hops: Vec<HopView>,
    pub end: EndView,
    pub review: ReviewView,
}

#[derive(Debug, Serialize)]
pub struct PointView {
    /// The project module, when the point lies inside the project.
    pub module: Option<String>,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct HopView {
    pub to: PointView,
    pub evidence: String,
    /// "established", "possible" or "proposed".
    pub support: &'static str,
    /// Present only for a possible hop with a stated condition.
    pub condition: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EndView {
    pub reached: bool,
    pub at: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewView {
    /// "judged", "deferred" or "unreviewed".
    pub status: &'static str,
    pub reason: Option<String>,
    pub primary: Option<String>,
    pub supporting: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Evidence {
    pub summary: String,
    pub unit: Option<String>,
    pub location: Option<LocationView>,
}

#[derive(Debug, Serialize)]
pub struct LocationView {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

pub fn view(map: &ProjectMap) -> View {
    let crates: Vec<Crate> = map
        .modules
        .iter()
        .map(|(key, module)| crate_view(map, key, module))
        .collect();
    let mut layers: Vec<String> = map.layers.iter().map(|layer| layer.0.clone()).collect();
    if crates.iter().any(|c| c.layer == UNASSIGNED_LAYER) {
        layers.push(UNASSIGNED_LAYER.into());
    }
    let edges: Vec<Edge> = map
        .dependencies
        .iter()
        .map(|(id, dependency)| Edge {
            from: id.from.0.clone(),
            to: id.to.0.clone(),
            roles: judged_roles(&dependency.description)
                .unwrap_or_else(|| vec!["unreviewed".into()]),
            other_roles: other_roles(&dependency.description),
            detail: review_reason(&dependency.description),
            evidence: dependency.evidence.iter().map(citation).collect(),
            review: review_view(&dependency.description),
        })
        .collect();
    View {
        meta: Meta {
            project: map.project.clone(),
            source: map.source.as_ref().map(|source| SourceMeta {
                graph_id: source.graph_id.0.clone(),
                revision: source.source_revision.0.clone(),
                target: source.target.clone(),
                configuration: source.configuration.clone(),
            }),
        },
        layers,
        groups: groups(&crates, &edges),
        crates,
        edges,
        routes: map
            .routes
            .iter()
            .map(|(id, route)| route_view(id, route))
            .collect(),
        evidence: map
            .evidence
            .iter()
            .map(|(id, e)| (citation(id), evidence_view(e)))
            .collect(),
        gaps: map.coverage_gaps.iter().map(evidence_view).collect(),
    }
}

fn crate_view(map: &ProjectMap, key: &ModuleId, module: &project_map::MapModule) -> Crate {
    let (kind, layer, concern) = match &module.description {
        Review::Judged { judgment, .. } => (
            kind_slug(&judgment.kind).to_string(),
            judgment
                .layer
                .as_ref()
                .filter(|layer| map.layers.contains(layer))
                .map_or_else(|| UNASSIGNED_LAYER.into(), |layer| layer.0.clone()),
            judgment.concern.clone(),
        ),
        _ => ("unreviewed".into(), UNASSIGNED_LAYER.into(), String::new()),
    };
    Crate {
        name: key.0.clone(),
        program: kind == kind_slug(&ModuleKind::Composition),
        kind,
        layer,
        concern,
        surface: module
            .surface
            .iter()
            .map(|item| SurfaceItem {
                name: item.name.clone(),
                kind: item_kind_slug(&item.kind),
                signature: item.signature.clone(),
                location: item.declaration.as_ref().map(location_view),
            })
            .collect(),
        review: review_view(&module.description),
    }
}

/// A contract groups the implementations in its own layer that implement it.
/// Each implementation joins at most one group, and a group head is never a partner.
fn groups(crates: &[Crate], edges: &[Edge]) -> BTreeMap<String, Vec<String>> {
    let by_name: BTreeMap<&str, &Crate> = crates.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut placed: Vec<&str> = vec![];
    for edge in edges
        .iter()
        .filter(|e| e.roles.iter().any(|r| r == "implements"))
    {
        let (Some(head), Some(partner)) = (
            by_name.get(edge.to.as_str()),
            by_name.get(edge.from.as_str()),
        ) else {
            continue;
        };
        let contract = kind_slug(&ModuleKind::Contract);
        if head.kind != contract
            || head.layer != partner.layer
            || placed.contains(&partner.name.as_str())
        {
            continue;
        }
        placed.push(&partner.name);
        groups
            .entry(head.name.clone())
            .or_default()
            .push(partner.name.clone());
    }
    groups.retain(|head, _| !placed.contains(&head.as_str()));
    for partners in groups.values_mut() {
        partners.sort();
    }
    groups
}

fn route_view(id: &RouteId, route: &project_map::MapRoute) -> Route {
    let mut modules: Vec<String> = vec![];
    let points =
        std::iter::once(route.origin.module()).chain(route.hops.iter().map(|hop| hop.to.module()));
    for module in points {
        if let Some(module) = module
            && modules.last() != Some(&module.0)
        {
            modules.push(module.0.clone());
        }
    }
    let roles = judged_roles(&route.description).unwrap_or_default();
    let injection = matches!(
        &route.description,
        Review::Judged { judgment, .. }
            if matches!(judgment.role, UseRole::FullInjection | UseRole::AdaptedInjection)
    ) && modules.len() >= 3;
    let first_role = route
        .hops
        .first()
        .and_then(|hop| hop.role.as_ref())
        .map(|role| role_slug(role).to_string())
        .or_else(|| roles.first().cloned())
        .unwrap_or_else(|| "unreviewed".into());
    let origin = point_view(&route.origin);
    let carry = match &route.origin {
        RoutePoint::Project(ProjectPoint {
            item: Some(item), ..
        }) => item.clone(),
        _ => origin.label.clone(),
    };
    Route {
        id: id.0.clone(),
        modules,
        origin,
        carry,
        first_role,
        roles,
        other_roles: other_roles(&route.description),
        injection,
        use_site: route.use_site.as_ref().map(citation),
        hops: route
            .hops
            .iter()
            .map(|hop| {
                let (support, condition) = match &hop.support {
                    HopSupport::Established => ("established", None),
                    HopSupport::Possible { condition } => ("possible", condition.clone()),
                    HopSupport::Proposed => ("proposed", None),
                };
                HopView {
                    to: point_view(&hop.to.point()),
                    evidence: citation(&hop.evidence),
                    support,
                    condition,
                    role: hop.role.as_ref().map(|role| role_slug(role).to_string()),
                }
            })
            .collect(),
        end: match &route.end {
            MapRouteEnd::ReachedUse => EndView {
                reached: true,
                at: None,
                reason: None,
            },
            MapRouteEnd::Stopped { at, reason } => EndView {
                reached: false,
                at: Some(citation(at)),
                reason: Some(reason.clone()),
            },
        },
        review: review_view(&route.description),
    }
}

fn point_view(point: &RoutePoint) -> PointView {
    match point {
        RoutePoint::Project(ProjectPoint { module, item }) => PointView {
            module: Some(module.0.clone()),
            label: match item {
                Some(item) => format!("{} · {item}", module.0),
                None => module.0.clone(),
            },
        },
        RoutePoint::Outside(OutsidePoint::External(name)) => PointView {
            module: None,
            label: format!("{name} (external)"),
        },
        RoutePoint::Outside(OutsidePoint::Unresolved(name)) => PointView {
            module: None,
            label: format!("{name} (unresolved)"),
        },
    }
}

fn judged_roles(review: &Review<project_map::RouteDescription>) -> Option<Vec<String>> {
    match review {
        Review::Judged { judgment, .. } => Some(
            std::iter::once(&judgment.role)
                .chain(&judgment.other_roles)
                .map(|role| role_slug(role).to_string())
                .collect(),
        ),
        _ => None,
    }
}

fn other_roles(review: &Review<project_map::RouteDescription>) -> Vec<String> {
    match review {
        Review::Judged { judgment, .. } => std::iter::once(&judgment.role)
            .chain(&judgment.other_roles)
            .filter_map(|role| match role {
                UseRole::Other(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

fn review_reason<T>(review: &Review<T>) -> String {
    match review {
        Review::Judged { reason, .. } | Review::Deferred { reason } => reason.clone(),
        Review::Unreviewed => String::new(),
    }
}

fn review_view<T>(review: &Review<T>) -> ReviewView {
    match review {
        Review::Unreviewed => ReviewView {
            status: "unreviewed",
            reason: None,
            primary: None,
            supporting: vec![],
        },
        Review::Deferred { reason } => ReviewView {
            status: "deferred",
            reason: Some(reason.clone()),
            primary: None,
            supporting: vec![],
        },
        Review::Judged {
            reason,
            primary,
            supporting,
            ..
        } => ReviewView {
            status: "judged",
            reason: Some(reason.clone()),
            primary: Some(citation(primary)),
            supporting: supporting.iter().map(citation).collect(),
        },
    }
}

fn evidence_view(evidence: &MapEvidence) -> Evidence {
    Evidence {
        summary: evidence.summary.clone(),
        unit: evidence.build_unit.as_ref().map(|unit| unit.0.clone()),
        location: evidence.location.as_ref().map(location_view),
    }
}

fn location_view(location: &Location) -> LocationView {
    LocationView {
        file: location.file.0.clone(),
        start: location.bytes.start,
        end: location.bytes.end,
    }
}

fn citation(id: &CitationId) -> String {
    id.0.clone()
}

fn item_kind_slug(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Type => "type",
        ItemKind::Value => "value",
        ItemKind::Function => "function",
        ItemKind::Method => "method",
        ItemKind::Module => "module",
    }
}

fn kind_slug(kind: &ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Logic => "logic",
        ModuleKind::Effect => "effect",
        ModuleKind::Composition => "composition",
        ModuleKind::Contract => "contract",
    }
}

/// The slugs the browser script draws. `Other` keeps its text in the detail.
fn role_slug(role: &UseRole) -> &'static str {
    match role {
        UseRole::DirectUse => "uses",
        UseRole::Implements => "implements",
        UseRole::Wires => "wires",
        UseRole::SuppliesInput => "supplies",
        UseRole::FullInjection => "injects",
        UseRole::AdaptedInjection => "adapts",
        UseRole::Incidental => "incidental",
        UseRole::Other(_) => "other",
    }
}

#[cfg(test)]
mod tests;
