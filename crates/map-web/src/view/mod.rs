//! The page's own representation of a map. The browser script reads this
//! shape; nothing outside map-web depends on it. Identifiers become plain
//! JSON strings here.

use project_map::{
    CitationId, DependencyId, EvidenceKind, HopSupport, ItemKind, Location, MapEvidence,
    MapRouteEnd, ModuleDescription, ModuleId, ModuleKind, OutsidePoint, ProjectMap, ProjectPoint,
    Review, RouteDescription, RouteId, RoutePoint, Status, UseRole,
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
    /// Contract modules keyed to the implementations drawn beneath them,
    /// in each view.
    pub groups: Groups,
    pub routes: Vec<Route>,
    pub evidence: BTreeMap<String, Evidence>,
    pub gaps: Vec<Evidence>,
}

/// The groups each view draws. A view shows some statuses, and takes an
/// update's new judgment except in `current`, so its groups can differ.
#[derive(Debug, Serialize)]
pub struct Groups {
    pub current: BTreeMap<String, Vec<String>>,
    pub planned: BTreeMap<String, Vec<String>>,
    pub changes: BTreeMap<String, Vec<String>>,
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
    /// "observed", "addition", "update" or "removal".
    pub status: &'static str,
    /// A ModuleKind slug, or "unreviewed" when the module has no judgment.
    pub kind: String,
    pub layer: String,
    pub concern: String,
    /// Draws the program's show and hide control on its box.
    pub program: bool,
    pub surface: Vec<SurfaceItem>,
    /// The module as found in code, or as proposed for an addition.
    pub review: ReviewView,
    /// A proposed update's new judgment.
    pub update: Option<CrateUpdate>,
}

/// A module's proposed update: its judgment after the update, and what changes.
#[derive(Debug, Serialize)]
pub struct CrateUpdate {
    pub kind: String,
    pub layer: String,
    pub concern: String,
    pub review: ReviewView,
    pub changes: Vec<Change>,
}

/// A dependency's or route's proposed update: its roles after the update,
/// and what changes.
#[derive(Debug, Serialize)]
pub struct ConnectionUpdate {
    pub roles: Vec<String>,
    pub other_roles: Vec<String>,
    pub detail: String,
    /// For a route: whether it is drawn as an injection after the update.
    pub injection: bool,
    pub review: ReviewView,
    pub changes: Vec<Change>,
}

/// One field an update changes, in words for the panel.
#[derive(Debug, Serialize)]
pub struct Change {
    pub field: &'static str,
    pub before: String,
    pub after: String,
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
    /// "observed", "addition", "update" or "removal".
    pub status: &'static str,
    pub from: String,
    pub to: String,
    /// Role slugs, primary first. An edge without a judgment has "unreviewed".
    pub roles: Vec<String>,
    /// The names of any `Other` roles, in order.
    pub other_roles: Vec<String>,
    pub detail: String,
    pub evidence: Vec<String>,
    /// The one-hop routes whose hop travels along this arrow: the direct
    /// uses it summarizes. The panel lists them under the arrow rather than
    /// among the routes.
    pub routes: Vec<String>,
    pub review: ReviewView,
    pub update: Option<ConnectionUpdate>,
}

#[derive(Debug, Serialize)]
pub struct Route {
    pub id: String,
    /// "observed", "addition", "update" or "removal".
    pub status: &'static str,
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
    /// The uses the route explains, one for each site that follows it.
    pub use_sites: Vec<String>,
    pub hops: Vec<HopView>,
    pub end: EndView,
    pub review: ReviewView,
    pub update: Option<ConnectionUpdate>,
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
    /// Every fact behind the step, one for each use site that takes it.
    pub evidence: Vec<String>,
    /// "established", "possible" or "proposed".
    pub support: &'static str,
    /// Present only for a possible hop with a stated condition.
    pub condition: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EndView {
    pub reached: bool,
    /// Where each use stopped; empty when the route reached its use.
    pub at: Vec<String>,
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
    /// "source", or "proposal" for an author's reasoning.
    pub kind: &'static str,
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
    let mut direct: BTreeMap<&DependencyId, Vec<String>> = BTreeMap::new();
    for (id, route) in &map.routes {
        if route.hops.len() == 1
            && let Some(dependency) = route.hop_dependency(0)
            && let Some((key, _)) = map.dependencies.get_key_value(&dependency)
        {
            direct.entry(key).or_default().push(id.0.clone());
        }
    }
    let edges: Vec<Edge> = map
        .dependencies
        .iter()
        .map(|(id, dependency)| Edge {
            status: status_slug(&dependency.status),
            from: id.from.0.clone(),
            to: id.to.0.clone(),
            roles: judged_roles(&dependency.description)
                .unwrap_or_else(|| vec!["unreviewed".into()]),
            other_roles: other_roles(&dependency.description),
            detail: review_reason(&dependency.description),
            evidence: dependency.evidence.iter().map(citation).collect(),
            routes: direct.remove(id).unwrap_or_default(),
            review: review_view(&dependency.description),
            update: match &dependency.status {
                Status::ProposedUpdate { proposed } => {
                    Some(connection_update(&dependency.description, proposed, false))
                }
                _ => None,
            },
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
        groups: Groups {
            current: groups(&crates, &edges, Show::Current),
            planned: groups(&crates, &edges, Show::Planned),
            changes: groups(&crates, &edges, Show::Changes),
        },
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

/// A module's kind slug, layer and concern as a review judges them.
fn module_facets(map: &ProjectMap, review: &Review<ModuleDescription>) -> (String, String, String) {
    match review {
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
    }
}

fn crate_view(map: &ProjectMap, key: &ModuleId, module: &project_map::MapModule) -> Crate {
    let (kind, layer, concern) = module_facets(map, &module.description);
    let update = match &module.status {
        Status::ProposedUpdate { proposed } => {
            let (new_kind, new_layer, new_concern) = module_facets(map, proposed);
            let changes = [
                ("kind", &kind, &new_kind),
                ("layer", &layer, &new_layer),
                ("concern", &concern, &new_concern),
            ]
            .into_iter()
            .filter(|(_, before, after)| before != after)
            .map(|(field, before, after)| Change {
                field,
                before: before.clone(),
                after: after.clone(),
            })
            .chain(state_change(&module.description, proposed))
            .collect();
            Some(CrateUpdate {
                kind: new_kind,
                layer: new_layer,
                concern: new_concern,
                review: review_view(proposed),
                changes,
            })
        }
        _ => None,
    };
    Crate {
        name: key.0.clone(),
        status: status_slug(&module.status),
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
        update,
    }
}

/// A connection's update. `injection`: whether a route is drawn as an
/// injection after the update; always false for a dependency.
fn connection_update(
    before: &Review<RouteDescription>,
    after: &Review<RouteDescription>,
    injection: bool,
) -> ConnectionUpdate {
    let words = |review: &Review<RouteDescription>| match review {
        Review::Judged { judgment, .. } => std::iter::once(&judgment.role)
            .chain(&judgment.other_roles)
            .map(role_words)
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    };
    let (old, new) = (words(before), words(after));
    let changes = (old != new)
        .then_some(Change {
            field: "roles",
            before: old,
            after: new,
        })
        .into_iter()
        .chain(state_change(before, after))
        .collect();
    ConnectionUpdate {
        roles: judged_roles(after).unwrap_or_else(|| vec!["unreviewed".into()]),
        other_roles: other_roles(after),
        detail: review_reason(after),
        injection,
        review: review_view(after),
        changes,
    }
}

/// The change of review state, such as judged to deferred, if any.
fn state_change<T>(before: &Review<T>, after: &Review<T>) -> Option<Change> {
    let (before, after) = (review_view(before).status, review_view(after).status);
    (before != after).then(|| Change {
        field: "review",
        before: before.into(),
        after: after.into(),
    })
}

/// The page's views, as the browser script names them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    /// What is in code: no additions, and updates as found in code.
    Current,
    /// The code after the proposals: no removals, and updates as proposed.
    Planned,
    /// Everything, with updates as proposed.
    Changes,
}

impl Show {
    fn shows(self, status: &str) -> bool {
        match self {
            Show::Current => status != "addition",
            Show::Planned => status != "removal",
            Show::Changes => true,
        }
    }
}

/// A contract groups the implementations in its own layer that implement it,
/// as `show` draws them. Each implementation joins at most one group, and a
/// group head is never a partner.
fn groups(crates: &[Crate], edges: &[Edge], show: Show) -> BTreeMap<String, Vec<String>> {
    // A module's kind and layer, and an edge's roles, as the view shows them.
    let facets = |c: &'_ Crate| match (&c.update, show) {
        (Some(update), Show::Planned | Show::Changes) => {
            (update.kind.clone(), update.layer.clone())
        }
        _ => (c.kind.clone(), c.layer.clone()),
    };
    let roles = |e: &'_ Edge| match (&e.update, show) {
        (Some(update), Show::Planned | Show::Changes) => update.roles.clone(),
        _ => e.roles.clone(),
    };
    let by_name: BTreeMap<&str, (String, String)> = crates
        .iter()
        .filter(|c| show.shows(c.status))
        .map(|c| (c.name.as_str(), facets(c)))
        .collect();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut placed: Vec<&str> = vec![];
    for edge in edges
        .iter()
        .filter(|e| show.shows(e.status) && roles(e).iter().any(|r| r == "implements"))
    {
        let (Some((head_kind, head_layer)), Some((_, partner_layer))) = (
            by_name.get(edge.to.as_str()),
            by_name.get(edge.from.as_str()),
        ) else {
            continue;
        };
        if head_kind != kind_slug(&ModuleKind::Contract)
            || head_layer != partner_layer
            || placed.contains(&edge.from.as_str())
        {
            continue;
        }
        placed.push(&edge.from);
        groups
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
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
    let injects = |review: &Review<RouteDescription>| {
        matches!(
            review,
            Review::Judged { judgment, .. }
                if matches!(judgment.role, UseRole::FullInjection | UseRole::AdaptedInjection)
        ) && modules.len() >= 3
    };
    let injection = injects(&route.description);
    let update = match &route.status {
        Status::ProposedUpdate { proposed } => Some(connection_update(
            &route.description,
            proposed,
            injects(proposed),
        )),
        _ => None,
    };
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
        status: status_slug(&route.status),
        modules,
        origin,
        carry,
        first_role,
        roles,
        other_roles: other_roles(&route.description),
        injection,
        use_sites: route.use_sites.iter().map(citation).collect(),
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
                    evidence: hop.evidence.iter().map(citation).collect(),
                    support,
                    condition,
                    role: hop.role.as_ref().map(|role| role_slug(role).to_string()),
                }
            })
            .collect(),
        end: match &route.end {
            MapRouteEnd::ReachedUse => EndView {
                reached: true,
                at: Vec::new(),
                reason: None,
            },
            MapRouteEnd::Stopped { at, reason } => EndView {
                reached: false,
                at: at.iter().map(citation).collect(),
                reason: Some(reason.clone()),
            },
        },
        review: review_view(&route.description),
        update,
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
        kind: match evidence.kind {
            EvidenceKind::Source => "source",
            EvidenceKind::Proposal => "proposal",
        },
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

fn status_slug<D>(status: &Status<D>) -> &'static str {
    match status {
        Status::Observed => "observed",
        Status::ProposedAddition => "addition",
        Status::ProposedUpdate { .. } => "update",
        Status::ProposedRemoval => "removal",
    }
}

/// A role in words, as the panel names it.
fn role_words(role: &UseRole) -> String {
    match role {
        UseRole::DirectUse => "direct use".into(),
        UseRole::Implements => "implements".into(),
        UseRole::Wires => "wires".into(),
        UseRole::SuppliesInput => "supplies input".into(),
        UseRole::FullInjection => "full injection".into(),
        UseRole::AdaptedInjection => "adapted injection".into(),
        UseRole::Incidental => "incidental use".into(),
        UseRole::Other(name) => format!("other: {name}"),
    }
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
