//! The map's evidence: one entry for each fact in the graph, and entries for
//! the routes, items and modules that judgments cite. Also turns graph
//! references into map citations without revealing what they name.

use crate::points::endpoint_label;
use project_graph::{
    Cited, Dispatch, Fact, FactId, FactKind, GraphRef, ImportUse, ItemId, ModuleId, ProjectGraph,
    ReceiverBasis, RouteEnd, RouteId, SourceSpan, UnitId,
};
use project_map::{CitationId, EvidenceKind, Location, MapEvidence, SourcePath};
use std::collections::BTreeMap;

pub(crate) struct Citations<'g> {
    graph: &'g ProjectGraph,
    /// Every reference the graph can resolve, with its map citation.
    index: BTreeMap<GraphRef, CitationId>,
    evidence: BTreeMap<CitationId, MapEvidence>,
}

impl<'g> Citations<'g> {
    /// Starts with an entry for each fact in the graph.
    pub(crate) fn new(graph: &'g ProjectGraph) -> Self {
        let mut index = BTreeMap::new();
        for id in graph.routes.keys() {
            index.insert(GraphRef::from(id.clone()), route_citation(id));
        }
        for id in graph.items.keys() {
            index.insert(GraphRef::from(id.clone()), item_citation(id));
        }
        for id in graph.facts.keys() {
            index.insert(GraphRef::from(id.clone()), fact_citation(id));
        }
        let evidence = graph
            .facts
            .iter()
            .map(|(id, fact)| (fact_citation(id), fact_evidence(graph, fact)))
            .collect();
        Citations {
            graph,
            index,
            evidence,
        }
    }

    /// The map citation for a reference, adding its entry to the evidence.
    /// `None` when the graph does not hold what it names.
    pub(crate) fn cite(&mut self, reference: &GraphRef) -> Option<CitationId> {
        let citation = self.index.get(reference)?.clone();
        if !self.evidence.contains_key(&citation) {
            let entry = match self.graph.resolve(reference)? {
                Cited::Route(route) => {
                    let end = match &route.end {
                        RouteEnd::ReachedUse => "reaches its use".to_string(),
                        RouteEnd::Stopped { reason, .. } => format!("stops: {reason}"),
                    };
                    MapEvidence {
                        kind: EvidenceKind::Source,
                        build_unit: None,
                        summary: format!(
                            "A route from {} with {} hop(s); it {end}.",
                            endpoint_label(self.graph, &route.origin),
                            route.hops.len()
                        ),
                        location: None,
                    }
                }
                Cited::Item(item) => MapEvidence {
                    kind: EvidenceKind::Source,
                    build_unit: None,
                    summary: format!(
                        "{} is declared in {}.",
                        item.signature.as_deref().unwrap_or(&item.name),
                        module_name(self.graph, &item.module)
                    ),
                    location: item.declaration.as_ref().map(location),
                },
                // Every fact already has its entry.
                Cited::Fact(_) => return Some(citation),
            };
            self.evidence.insert(citation.clone(), entry);
        }
        Some(citation)
    }

    /// The citation for a fact. A fact the graph does not hold gets an entry
    /// saying so, and `false`.
    pub(crate) fn fact(&mut self, id: &FactId) -> (CitationId, bool) {
        let citation = fact_citation(id);
        let held = self.graph.facts.contains_key(id);
        if !held {
            self.evidence
                .entry(citation.clone())
                .or_insert_with(|| MapEvidence {
                    kind: EvidenceKind::Source,
                    build_unit: None,
                    summary: format!("Fact {} is missing from the graph.", id.0),
                    location: None,
                });
        }
        (citation, held)
    }

    /// The citation for a module itself, adding its entry to the evidence.
    pub(crate) fn module(&mut self, id: &ModuleId) -> CitationId {
        let citation = CitationId(format!("module:{}", id.0));
        let root = self
            .graph
            .modules
            .get(id)
            .map(|module| module.root.as_str())
            .unwrap_or("an unknown root");
        let summary = format!("Module {} at {root}.", module_name(self.graph, id));
        self.evidence
            .entry(citation.clone())
            .or_insert_with(|| MapEvidence {
                kind: EvidenceKind::Source,
                build_unit: None,
                summary,
                location: None,
            });
        citation
    }

    pub(crate) fn into_evidence(self) -> BTreeMap<CitationId, MapEvidence> {
        self.evidence
    }
}

/// The citation of a route's own entry. The entry exists only when a
/// judgment cites the route.
pub(crate) fn route_citation(id: &RouteId) -> CitationId {
    CitationId(format!("route:{}", id.0))
}

fn item_citation(id: &ItemId) -> CitationId {
    CitationId(format!("item:{}", id.0))
}

pub(crate) fn fact_citation(id: &FactId) -> CitationId {
    CitationId(format!("fact:{}", id.0))
}

pub(crate) fn unit(id: &UnitId) -> project_map::UnitId {
    project_map::UnitId(id.0.clone())
}

pub(crate) fn location(span: &SourceSpan) -> Location {
    Location {
        file: SourcePath(span.file.clone()),
        bytes: span.bytes.clone(),
    }
}

fn module_name<'a>(graph: &'a ProjectGraph, id: &'a ModuleId) -> &'a str {
    graph
        .modules
        .get(id)
        .map(|module| module.name.as_str())
        .unwrap_or(&id.0)
}

fn fact_evidence(graph: &ProjectGraph, fact: &Fact) -> MapEvidence {
    MapEvidence {
        kind: EvidenceKind::Source,
        build_unit: Some(unit(&fact.unit)),
        summary: fact_summary(graph, fact),
        location: fact.site.as_ref().map(location),
    }
}

/// One sentence saying what the fact records.
fn fact_summary(graph: &ProjectGraph, fact: &Fact) -> String {
    let subject = endpoint_label(graph, &fact.subject);
    let object = endpoint_label(graph, &fact.object);
    match &fact.kind {
        FactKind::DeclaredDependency => format!("{subject} declares a dependency on {object}."),
        FactKind::Import { local_name, usage } => {
            let usage = match usage {
                ImportUse::Unused => "unused",
                ImportUse::TypeOnly => "used only as a type",
                ImportUse::Value => "used as a value",
                ImportUse::Undetermined => "use undetermined",
            };
            format!("{subject} imports {object} as {local_name} ({usage}).")
        }
        FactKind::Reexport { exported_name } => {
            format!("{subject} re-exports {object} as {exported_name}.")
        }
        FactKind::TypeReference => format!("{subject} mentions the type {object}."),
        FactKind::ValueReference => format!("{subject} refers to {object}."),
        FactKind::Implements => format!("{subject} implements {object}."),
        FactKind::Call(dispatch) => {
            let dispatch = match dispatch {
                Dispatch::Direct => "directly",
                Dispatch::SelectedImplementation => "through a selected implementation",
                Dispatch::Contract => "through a contract",
                Dispatch::Indirect => "indirectly",
                Dispatch::Foreign => "across a foreign interface",
            };
            format!("{subject} calls {object} {dispatch}.")
        }
        FactKind::ReceiverBound { call, basis } => {
            let basis = match basis {
                ReceiverBasis::SelectedByCompiler => "selected by the compiler",
                ReceiverBasis::UniqueInClosedBuild => "the only one in the build",
                ReceiverBasis::BuildCandidate => "a candidate in the build",
            };
            format!(
                "{object} receives the call at fact {} as {subject} ({basis}).",
                call.0
            )
        }
        FactKind::Construct => format!("{subject} constructs {object}."),
        FactKind::Store => format!("{subject} is stored into {object}."),
        FactKind::Read => format!("{subject} is read into {object}."),
        FactKind::Pass => format!("{subject} is passed to {object}."),
        FactKind::Return => format!("{subject} is returned to {object}."),
        FactKind::Coerce => format!("{subject} is coerced into {object}."),
        FactKind::Capture => format!("{subject} is captured by {object}."),
    }
}
