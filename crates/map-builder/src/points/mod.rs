//! Where a graph endpoint lies on the map: in which project module, at which
//! item, or outside the project.

use project_graph::{Endpoint, ItemId, ModuleId, ProjectGraph};
use project_map::{OutsidePoint, ProjectPoint, RoutePoint};

/// The project module holding the endpoint. `None` for points outside the
/// project, and for modules and items the graph does not hold.
pub(crate) fn module_of<'g>(
    graph: &'g ProjectGraph,
    endpoint: &'g Endpoint,
) -> Option<&'g ModuleId> {
    let module = match endpoint {
        Endpoint::Module(id) => id,
        Endpoint::Item(id) | Endpoint::Place { owner: id, .. } => &graph.items.get(id)?.module,
        Endpoint::External(_) | Endpoint::Unresolved(_) => return None,
    };
    graph.modules.contains_key(module).then_some(module)
}

/// The item the endpoint names, if it names one.
pub(crate) fn item_of(endpoint: &Endpoint) -> Option<&ItemId> {
    match endpoint {
        Endpoint::Item(id) | Endpoint::Place { owner: id, .. } => Some(id),
        Endpoint::Module(_) | Endpoint::External(_) | Endpoint::Unresolved(_) => None,
    }
}

/// The endpoint as a map point. A module or item the graph does not hold is
/// an unresolved point outside the project.
pub(crate) fn route_point(graph: &ProjectGraph, endpoint: &Endpoint) -> RoutePoint {
    match project_point(graph, endpoint) {
        Some(point) => RoutePoint::Project(point),
        None => RoutePoint::Outside(outside_point(graph, endpoint)),
    }
}

pub(crate) fn project_point(graph: &ProjectGraph, endpoint: &Endpoint) -> Option<ProjectPoint> {
    let module = module_of(graph, endpoint)?;
    let item = match endpoint {
        Endpoint::Module(_) => None,
        Endpoint::Item(id) => Some(item_label(graph, id)),
        Endpoint::Place { owner, projection } => {
            Some(format!("{}.{projection}", item_label(graph, owner)))
        }
        Endpoint::External(_) | Endpoint::Unresolved(_) => return None,
    };
    Some(ProjectPoint {
        module: project_map::ModuleId(module.0.clone()),
        item,
    })
}

pub(crate) fn outside_point(graph: &ProjectGraph, endpoint: &Endpoint) -> OutsidePoint {
    match endpoint {
        Endpoint::External(name) => OutsidePoint::External(name.clone()),
        Endpoint::Unresolved(name) => OutsidePoint::Unresolved(name.clone()),
        Endpoint::Module(_) | Endpoint::Item(_) | Endpoint::Place { .. } => {
            OutsidePoint::Unresolved(format!(
                "{} (missing from the graph)",
                endpoint_label(graph, endpoint)
            ))
        }
    }
}

/// How the endpoint reads in evidence summaries.
pub(crate) fn endpoint_label(graph: &ProjectGraph, endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Module(id) => graph
            .modules
            .get(id)
            .map(|module| module.name.clone())
            .unwrap_or_else(|| id.0.clone()),
        Endpoint::Item(id) => item_label(graph, id),
        Endpoint::Place { owner, projection } => {
            format!("{}.{projection}", item_label(graph, owner))
        }
        Endpoint::External(name) => name.clone(),
        Endpoint::Unresolved(name) => format!("unresolved {name}"),
    }
}

/// The item's name, or its identifier when the graph does not hold it.
pub(crate) fn item_label(graph: &ProjectGraph, id: &ItemId) -> String {
    graph
        .items
        .get(id)
        .map(|item| item.name.clone())
        .unwrap_or_else(|| id.0.clone())
}
