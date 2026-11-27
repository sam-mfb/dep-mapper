//! The map's modules: each graph module with the surface it offers other
//! modules and the evaluation's judgment of it.

use crate::BuildProblem;
use crate::citations::{Citations, location};
use graph_evaluator::{Decision, GraphEvaluation, ModuleJudgment};
use project_graph::{ItemKind, ModuleId, ProjectGraph, Visibility};
use project_map::{
    LayerName, MapModule, ModuleDescription, ModuleKind, Review, Status, SurfaceItem,
};
use std::collections::BTreeMap;

/// Every module in the graph. Reports judgments of modules the graph does
/// not have, citations it cannot resolve and clusters the evaluation does
/// not list.
pub(crate) fn build(
    graph: &ProjectGraph,
    evaluation: &GraphEvaluation,
    citations: &mut Citations,
    problems: &mut Vec<BuildProblem>,
) -> BTreeMap<project_map::ModuleId, MapModule> {
    for id in evaluation.modules.keys() {
        if !graph.modules.contains_key(id) {
            problems.push(BuildProblem::UnknownModule(id.clone()));
        }
    }
    graph
        .modules
        .iter()
        .map(|(id, module)| {
            let description = match evaluation.modules.get(id) {
                None => Review::Unreviewed,
                Some(Decision::Deferred { reason }) => Review::Deferred {
                    reason: reason.clone(),
                },
                Some(Decision::Judged {
                    judgment,
                    reason,
                    evidence,
                }) => {
                    let mut cited = vec![];
                    for reference in evidence {
                        match citations.cite(reference) {
                            Some(citation) if !cited.contains(&citation) => cited.push(citation),
                            Some(_) => {}
                            None => problems.push(BuildProblem::UnresolvedModuleCitation {
                                module: id.clone(),
                                reference: reference.clone(),
                            }),
                        }
                    }
                    let primary = if cited.is_empty() {
                        citations.module(id)
                    } else {
                        cited.remove(0)
                    };
                    Review::Judged {
                        judgment: description(id, judgment, evaluation, problems),
                        reason: reason.clone(),
                        primary,
                        supporting: cited,
                    }
                }
            };
            let map_module = MapModule {
                status: Status::Observed,
                name: module.name.clone(),
                surface: surface(graph, id),
                description,
            };
            (project_map::ModuleId(id.0.clone()), map_module)
        })
        .collect()
}

fn description(
    id: &ModuleId,
    judgment: &ModuleJudgment,
    evaluation: &GraphEvaluation,
    problems: &mut Vec<BuildProblem>,
) -> ModuleDescription {
    let layer = if evaluation.clusters.contains(&judgment.cluster) {
        Some(LayerName(judgment.cluster.0.clone()))
    } else {
        problems.push(BuildProblem::UnlistedCluster {
            module: id.clone(),
            cluster: judgment.cluster.clone(),
        });
        None
    };
    let kind = match judgment.kind {
        graph_evaluator::ModuleKind::Logic => ModuleKind::Logic,
        graph_evaluator::ModuleKind::Effect => ModuleKind::Effect,
        graph_evaluator::ModuleKind::Composition => ModuleKind::Composition,
        graph_evaluator::ModuleKind::Contract => ModuleKind::Contract,
    };
    ModuleDescription {
        concern: judgment.concern.clone(),
        kind,
        layer,
    }
}

/// The module's consumer-facing declarations, in the graph's item order:
/// the exported types (traits and interfaces among them), values and
/// functions the grapher records. Methods and submodules are left out; the
/// graph keeps them, and routes still cite them.
fn surface(graph: &ProjectGraph, module: &ModuleId) -> Vec<SurfaceItem> {
    graph
        .items
        .values()
        .filter(|item| &item.module == module && item.visibility == Visibility::Exported)
        .filter_map(|item| {
            let kind = match item.kind {
                ItemKind::Type => project_map::ItemKind::Type,
                ItemKind::Value => project_map::ItemKind::Value,
                ItemKind::Function => project_map::ItemKind::Function,
                ItemKind::Method | ItemKind::Module => return None,
            };
            Some(SurfaceItem {
                name: item.name.clone(),
                kind,
                signature: item.signature.clone(),
                declaration: item.declaration.as_ref().map(location),
            })
        })
        .collect()
}
