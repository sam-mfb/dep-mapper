//! Presents a graph as a workbook: pieces an agent can read one at a time,
//! and an index of every handle an answer may cite. The workbook holds only
//! text and identifiers, so it can be written out and read back without the
//! graph. Answers are checked against it.
//!
//! Pieces come in three sorts. A module piece covers one module and asks for
//! its decision. The clusters piece asks for the cluster list. A routes piece
//! covers the routes between two modules, or the routes that stay in one
//! module or leave it for points outside the project, and asks for the
//! relationships those routes carry.

mod render;

use project_graph::{Endpoint, FactKind, ModuleId, ProjectGraph, RouteId, SourceSpan, UnitId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub use render::{ByteOffsets, SourceLines};

/// A graph presented for evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workbook {
    /// The `GraphId` of the presented graph.
    pub graph: String,
    pub project: String,
    /// Module pieces, then the clusters piece, then routes pieces.
    pub pieces: Vec<Piece>,
    /// Every module, by identifier, with its name.
    pub modules: BTreeMap<String, String>,
    /// Every handle an answer may cite as evidence.
    pub refs: BTreeMap<String, RefInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    pub id: PieceId,
    pub title: String,
    pub subject: PieceSubject,
    /// The piece's text. A long piece has several pages.
    pub pages: Vec<String>,
}

/// Names a piece, such as `module/core`, `clusters` or `routes/core+web`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PieceId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sort", rename_all = "kebab-case")]
pub enum PieceSubject {
    Module {
        module: String,
    },
    Clusters,
    Routes {
        /// The one or two modules the piece is about.
        modules: Vec<String>,
        /// The handles of its routes.
        routes: Vec<String>,
    },
}

/// What the checks need to know about a citable handle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefInfo {
    /// Project modules the cited thing lies in or passes through.
    pub modules: BTreeSet<String>,
    /// The kinds of the facts it is or contains, such as `implements`.
    pub fact_kinds: BTreeSet<String>,
    /// A full description for `show`.
    pub detail: String,
    /// A shorter description, for a route whose full one names more facts
    /// than a piece lists. Pieces show it in place of `detail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl RefInfo {
    /// The description to show among others: the summary when there is one.
    pub fn brief_detail(&self) -> &str {
        self.summary.as_deref().unwrap_or(&self.detail)
    }
}

/// How to present a graph.
pub struct Presentation<'a> {
    /// The most routes listed on one page of a piece.
    pub page_size: usize,
    pub source: &'a dyn SourceLines,
    /// How a reader invokes the answer commands, such as `srs eval`. With
    /// `None`, pieces hold the evidence and the request without command lines.
    pub command: Option<&'a str>,
}

impl PieceId {
    pub fn module(module: &str) -> PieceId {
        PieceId(format!("module/{module}"))
    }

    pub fn clusters() -> PieceId {
        PieceId("clusters".into())
    }
}

impl Workbook {
    pub fn piece(&self, id: &PieceId) -> Option<&Piece> {
        self.pieces.iter().find(|piece| &piece.id == id)
    }

    /// The routes piece that holds a route, by the route's handle.
    pub fn piece_of_route(&self, route: &str) -> Option<&Piece> {
        self.pieces.iter().find(|piece| match &piece.subject {
            PieceSubject::Routes { routes, .. } => routes.iter().any(|r| r == route),
            _ => false,
        })
    }
}

pub fn present(graph: &ProjectGraph, presentation: &Presentation) -> Workbook {
    let text = render::Renderer::new(graph, presentation.source, presentation.command);
    let refs = refs(graph, &text);
    let groups = route_groups(graph);

    let piece_of: BTreeMap<RouteId, String> = groups
        .iter()
        .flat_map(|(modules, routes)| {
            let id = routes_piece_id(modules);
            routes.iter().map(move |route| (route.clone(), id.clone()))
        })
        .collect();

    let mut pieces = Vec::new();
    for (id, module) in &graph.modules {
        let touching: Vec<&RouteId> = graph
            .routes
            .keys()
            .filter(|route| refs[&route_handle(route)].modules.contains(&id.0))
            .collect();
        pieces.push(Piece {
            id: PieceId::module(&id.0),
            title: format!("Module {}", module.name),
            subject: PieceSubject::Module {
                module: id.0.clone(),
            },
            pages: text.module_pages(id, &touching, &piece_of, presentation.page_size),
        });
    }
    pieces.push(Piece {
        id: PieceId::clusters(),
        title: "Clusters".into(),
        subject: PieceSubject::Clusters,
        pages: vec![text.clusters_page()],
    });
    for (modules, routes) in &groups {
        let names: Vec<String> = modules.iter().map(|m| text.module_name(m)).collect();
        pieces.push(Piece {
            id: PieceId(routes_piece_id(modules)),
            title: match names.as_slice() {
                [one] => format!("Routes within or leaving {one}"),
                _ => format!("Routes between {}", names.join(" and ")),
            },
            subject: PieceSubject::Routes {
                modules: modules.iter().map(|m| m.0.clone()).collect(),
                routes: routes.iter().map(route_handle).collect(),
            },
            pages: text.routes_pages(modules, routes, presentation.page_size),
        });
    }

    Workbook {
        graph: graph.id.0.clone(),
        project: graph.build.project.clone(),
        pieces,
        modules: graph
            .modules
            .iter()
            .map(|(id, module)| (id.0.clone(), module.name.clone()))
            .collect(),
        refs,
    }
}

fn routes_piece_id(modules: &[ModuleId]) -> String {
    let names: Vec<&str> = modules.iter().map(|m| m.0.as_str()).collect();
    format!("routes/{}", names.join("+"))
}

fn route_handle(route: &RouteId) -> String {
    format!("route:{}", route.0)
}

fn refs(graph: &ProjectGraph, text: &render::Renderer) -> BTreeMap<String, RefInfo> {
    let mut refs = BTreeMap::new();
    for (id, item) in &graph.items {
        refs.insert(
            format!("item:{}", id.0),
            RefInfo {
                modules: BTreeSet::from([item.module.0.clone()]),
                fact_kinds: BTreeSet::new(),
                detail: text.item_detail(id),
                summary: None,
            },
        );
    }
    for (id, fact) in &graph.facts {
        let mut modules = BTreeSet::new();
        modules.extend(unit_module(graph, &fact.unit));
        modules.extend(endpoint_module(graph, &fact.subject));
        modules.extend(endpoint_module(graph, &fact.object));
        refs.insert(
            format!("fact:{}", id.0),
            RefInfo {
                modules: modules.into_iter().map(|m| m.0).collect(),
                fact_kinds: BTreeSet::from([fact_kind_slug(&fact.kind).to_string()]),
                detail: text.fact_detail(id),
                summary: None,
            },
        );
    }
    for (id, route) in &graph.routes {
        let mut modules: BTreeSet<ModuleId> = route_modules(graph, id).into_iter().collect();
        let mut fact_kinds = BTreeSet::new();
        let facts = route
            .hops
            .iter()
            .flat_map(|hop| &hop.facts)
            .chain(&route.use_sites);
        for fact in facts.filter_map(|id| graph.facts.get(id)) {
            modules.extend(unit_module(graph, &fact.unit));
            fact_kinds.insert(fact_kind_slug(&fact.kind).to_string());
        }
        let detail = text.route_detail(id, render::Length::Full);
        refs.insert(
            route_handle(id),
            RefInfo {
                modules: modules.into_iter().map(|m| m.0).collect(),
                fact_kinds,
                summary: Some(text.route_detail(id, render::Length::Summary))
                    .filter(|summary| *summary != detail),
                detail,
            },
        );
    }
    refs
}

/// Groups routes by the first and last project modules along them. A route
/// with one project module along it is grouped under that module alone. A
/// route with none is grouped under the module of its facts' build units.
fn route_groups(graph: &ProjectGraph) -> BTreeMap<Vec<ModuleId>, Vec<RouteId>> {
    let mut groups: BTreeMap<Vec<ModuleId>, Vec<RouteId>> = BTreeMap::new();
    for (id, route) in &graph.routes {
        let along = route_modules(graph, id);
        let key = match (along.first(), along.last()) {
            (Some(first), Some(last)) if first != last => {
                let mut pair = vec![first.clone(), last.clone()];
                pair.sort();
                pair
            }
            (Some(only), _) => vec![only.clone()],
            _ => route
                .hops
                .iter()
                .flat_map(|hop| &hop.facts)
                .filter_map(|fact| graph.facts.get(fact))
                .find_map(|fact| unit_module(graph, &fact.unit))
                .into_iter()
                .collect(),
        };
        groups.entry(key).or_default().push(id.clone());
    }
    groups
}

/// Project modules along a route's endpoints, in order, without repeats in a row.
fn route_modules(graph: &ProjectGraph, id: &RouteId) -> Vec<ModuleId> {
    let route = &graph.routes[id];
    let mut along: Vec<ModuleId> = Vec::new();
    let points = std::iter::once(&route.origin).chain(route.hops.iter().map(|hop| &hop.to));
    for module in points.filter_map(|point| endpoint_module(graph, point)) {
        if along.last() != Some(&module) {
            along.push(module);
        }
    }
    along
}

fn endpoint_module(graph: &ProjectGraph, endpoint: &Endpoint) -> Option<ModuleId> {
    match endpoint {
        Endpoint::Module(id) => Some(id.clone()),
        Endpoint::Item(id) | Endpoint::Place { owner: id, .. } => {
            graph.items.get(id).map(|item| item.module.clone())
        }
        Endpoint::External(_) | Endpoint::Unresolved(_) => None,
    }
}

fn unit_module(graph: &ProjectGraph, unit: &UnitId) -> Option<ModuleId> {
    graph.units.get(unit).map(|unit| unit.module.clone())
}

/// A short name for a fact kind, used in the index and in checks.
pub fn fact_kind_slug(kind: &FactKind) -> &'static str {
    match kind {
        FactKind::DeclaredDependency => "declared-dependency",
        FactKind::Import { .. } => "import",
        FactKind::Reexport { .. } => "reexport",
        FactKind::TypeReference => "type-reference",
        FactKind::ValueReference => "value-reference",
        FactKind::Implements => "implements",
        FactKind::Call(_) => "call",
        FactKind::ReceiverBound { .. } => "receiver-bound",
        FactKind::Construct => "construct",
        FactKind::Store => "store",
        FactKind::Read => "read",
        FactKind::Pass => "pass",
        FactKind::Return => "return",
        FactKind::Coerce => "coerce",
        FactKind::Capture => "capture",
    }
}

/// Where a span lies, for people to read.
pub(crate) fn span_text(source: &dyn SourceLines, span: &SourceSpan) -> String {
    match source.lines(span) {
        Some((first, last)) if first == last => format!("{}:{first}", span.file),
        Some((first, last)) => format!("{}:{first}-{last}", span.file),
        None => format!(
            "{} bytes {}..{}",
            span.file, span.bytes.start, span.bytes.end
        ),
    }
}

#[cfg(test)]
mod tests;
