//! Builds a `ProjectMap` from a `ProjectGraph` and the `GraphEvaluation` of
//! that graph. The transformation is pure: it reads both inputs and returns
//! the map with a list of the problems it found joining them.
//!
//! Every module, route and coverage gap in the graph appears on the map,
//! whether or not the evaluation mentions it. Unreviewed parts stay
//! unreviewed, and possible hops and stopped traces keep their support and
//! reasons. Judgments are matched to their subjects through the graph
//! references they cite:
//!
//! - A module judgment applies to the module it is keyed by.
//! - A relationship applies to each route it cites, to each hop one of whose
//!   facts it cites, and to each dependency whose facts it cites or whose routes it
//!   cites, when every hop of that route between two modules travels along
//!   that one dependency.
//!
//! When several relationships apply to one subject, the first in the
//! evaluation's order gives the role and the rest give other roles.
//! A hop has a single role, so only the first relationship citing one of its
//! facts applies to it.
//!
//! Dependencies come from the graph's facts. A fact whose subject uses its
//! object, such as an import, a reference, an implementation or a call, makes
//! a dependency from the subject's module to the object's module. A value
//! move (construct, read, store, pass, return, coerce or capture) makes none
//! by itself. A route hop
//! between two modules travels along the dependency that joins them, and adds
//! its facts to that dependency's evidence. When no use fact joins them, the
//! hop's own facts decide the direction: after a read or a return, the
//! receiver uses the sender; after any other move, the sender uses the
//! receiver. When both directions exist, the receiver using the sender wins.
//!
//! All map citations name graph things: `fact:<id>` for each fact, and
//! `route:<id>`, `item:<id>` and `module:<id>` entries for routes, items and
//! modules that judgments cite. A module judgment that cites nothing in the
//! graph rests on its module's entry.

mod citations;
mod dependencies;
mod modules;
mod points;
mod reviews;
mod routes;

use citations::Citations;
use graph_evaluator::{ClusterName, GraphEvaluation};
use project_graph::{GraphId, GraphRef, ModuleId, ProjectGraph};
use project_map::{
    DuplicateLayer, LayerName, Layers, MapEvidence, MapSource, ProjectMap, SourceRevision,
};
use thiserror::Error;

/// The map built from a graph and its evaluation, and what did not fit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapBuild {
    pub map: ProjectMap,
    /// Problems joining the evaluation to the graph.
    pub problems: Vec<BuildProblem>,
}

/// Something the builder could not match. The map is built without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildProblem {
    /// The evaluation judges a module the graph does not have.
    UnknownModule(ModuleId),
    /// A module judgment cites something the graph does not have.
    UnresolvedModuleCitation {
        module: ModuleId,
        reference: GraphRef,
    },
    /// A module judgment names a cluster the evaluation does not list. The
    /// module is drawn without a layer.
    UnlistedCluster {
        module: ModuleId,
        cluster: ClusterName,
    },
    /// A relationship cites something the graph does not have. `relationship`
    /// is its index in the evaluation's relationships.
    UnresolvedRelationshipCitation {
        relationship: usize,
        reference: GraphRef,
    },
    /// A relationship applies to no route, hop or dependency on the map.
    UnplacedRelationship { relationship: usize },
    /// The evaluation lists a cluster more than once. The map keeps its
    /// first place in the reading order.
    DuplicateCluster(ClusterName),
    /// The graph refers to something it does not hold: `cited_by` names the
    /// route or fact holding the reference.
    IncompleteGraph {
        cited_by: GraphRef,
        missing: GraphRef,
    },
}

/// The evaluation reviews a different graph snapshot than the one supplied.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("the evaluation reviews graph {evaluated:?}, not graph {graph:?}")]
pub struct GraphMismatch {
    pub graph: GraphId,
    pub evaluated: GraphId,
}

/// Builds the map of `graph` as `evaluation` reviews it.
pub fn build_map(
    graph: &ProjectGraph,
    evaluation: &GraphEvaluation,
) -> Result<MapBuild, GraphMismatch> {
    if evaluation.graph != graph.id {
        return Err(GraphMismatch {
            graph: graph.id.clone(),
            evaluated: evaluation.graph.clone(),
        });
    }
    let mut problems = vec![];
    let mut citations = Citations::new(graph);
    let modules = modules::build(graph, evaluation, &mut citations, &mut problems);
    let reviews = reviews::Reviews::new(evaluation, &mut citations, &mut problems);
    let mut dependencies = dependencies::Dependencies::from_facts(graph, &mut problems);
    let routes = routes::build(
        graph,
        &reviews,
        &mut dependencies,
        &mut citations,
        &mut problems,
    );
    let dependencies = dependencies.into_map(&routes, &reviews);
    problems.extend(reviews.unplaced(&routes, &dependencies));
    let layers = layers(evaluation.clusters.iter(), &mut problems);
    let coverage_gaps = graph
        .gaps
        .iter()
        .map(|gap| MapEvidence {
            kind: project_map::EvidenceKind::Source,
            build_unit: gap.unit.as_ref().map(citations::unit),
            summary: gap.reason.clone(),
            location: gap.site.as_ref().map(citations::location),
        })
        .collect();
    let map = ProjectMap {
        project: graph.build.project.clone(),
        source: Some(MapSource {
            graph_id: project_map::GraphId(graph.id.0.clone()),
            source_revision: SourceRevision(graph.build.source_revision.clone()),
            target: graph.build.target.clone(),
            configuration: graph.build.configuration.clone(),
        }),
        layers,
        modules,
        dependencies,
        routes,
        evidence: citations.into_evidence(),
        coverage_gaps,
    };
    Ok(MapBuild { map, problems })
}

/// The map's layers: the clusters in reading order, each once.
fn layers<'a>(
    clusters: impl Iterator<Item = &'a ClusterName>,
    problems: &mut Vec<BuildProblem>,
) -> Layers {
    let mut names: Vec<LayerName> = vec![];
    for cluster in clusters {
        let name = LayerName(cluster.0.clone());
        if names.contains(&name) {
            problems.push(BuildProblem::DuplicateCluster(cluster.clone()));
        } else {
            names.push(name);
        }
    }
    match Layers::new(names) {
        Ok(layers) => layers,
        // Each name was added once above, so this is not expected; the map
        // is still built, without layers, and the problem is reported.
        Err(DuplicateLayer(layer)) => {
            problems.push(BuildProblem::DuplicateCluster(ClusterName(layer.0)));
            Layers::empty()
        }
    }
}

#[cfg(test)]
mod tests;
