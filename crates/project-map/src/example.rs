//! dep-mapper's own proposed architecture as a `ProjectMap`, for consumers
//! that need a realistic map to display or store. It records the module map
//! in `docs/architecture/map.html` as it stood when this example was written.
//! No source was analyzed, so evidence entries describe the proposal and
//! carry no source locations.

mod tables;

use crate::*;
use std::collections::BTreeMap;

struct ModuleRow {
    key: &'static str,
    layer: &'static str,
    kind: ModuleKind,
    concern: &'static str,
    ownership: &'static str,
    /// Name, item kind and optional signature.
    surface: Vec<(&'static str, ItemKind, Option<&'static str>)>,
}

struct DependencyRow {
    from: &'static str,
    to: &'static str,
    role: UseRole,
    detail: &'static str,
}

/// A route from a supplier, through the program, to a receiver.
struct RouteRow {
    key: &'static str,
    supplier: &'static str,
    /// The supplier's item that travels along the route.
    carries: &'static str,
    program: &'static str,
    /// The role of the program's use of the supplier.
    first_role: UseRole,
    receiver: &'static str,
    role: UseRole,
    /// Present when the route applies only under this condition.
    condition: Option<&'static str>,
    note: &'static str,
}

fn module_citation(key: &str) -> CitationId {
    CitationId(format!("module:{key}"))
}

fn dependency_citation(from: &str, to: &str) -> CitationId {
    CitationId(format!("dependency:{from}>{to}"))
}

fn judged<T>(
    judgment: T,
    reason: &str,
    primary: CitationId,
    supporting: Vec<CitationId>,
) -> Review<T> {
    Review::Judged {
        judgment,
        reason: reason.into(),
        primary,
        supporting,
    }
}

fn hop_target(module: &str, link: HopLink) -> HopTarget {
    HopTarget::Project {
        point: ProjectPoint {
            module: ModuleId(module.into()),
            item: None,
        },
        link,
    }
}

fn proposal_evidence(summary: String) -> MapEvidence {
    MapEvidence {
        build_unit: None,
        summary,
        location: None,
    }
}

/// The dep-mapper module map. Every route reaches its use; the TypeScript
/// reader's hops are possible only for a TypeScript project.
pub fn dep_mapper() -> ProjectMap {
    let mut evidence = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for row in tables::modules() {
        let citation = module_citation(row.key);
        evidence.insert(
            citation.clone(),
            proposal_evidence(format!(
                "{} is a proposed module in docs/architecture/decisions.md.",
                row.key
            )),
        );
        let surface = row
            .surface
            .into_iter()
            .map(|(name, kind, signature)| SurfaceItem {
                name: name.into(),
                kind,
                signature: signature.map(Into::into),
                declaration: None,
            })
            .collect();
        let description = ModuleDescription {
            concern: row.concern.into(),
            kind: row.kind,
            layer: Some(LayerName(row.layer.into())),
        };
        modules.insert(
            ModuleId(row.key.into()),
            MapModule {
                name: row.key.into(),
                surface,
                description: judged(description, row.ownership, citation, vec![]),
            },
        );
    }

    let dependencies = tables::dependencies()
        .into_iter()
        .map(|row| {
            let citation = dependency_citation(row.from, row.to);
            evidence.insert(citation.clone(), proposal_evidence(row.detail.into()));
            let description = RouteDescription {
                role: row.role,
                other_roles: vec![],
            };
            let id = DependencyId {
                from: ModuleId(row.from.into()),
                to: ModuleId(row.to.into()),
            };
            let dependency = MapDependency {
                evidence: vec![citation.clone()],
                description: judged(description, row.detail, citation, vec![]),
            };
            (id, dependency)
        })
        .collect();

    let routes = tables::routes()
        .into_iter()
        .map(|row| {
            let support = match row.condition {
                Some(condition) => HopSupport::Possible {
                    condition: Some(condition.into()),
                },
                None => HopSupport::Established,
            };
            let obtained = dependency_citation(row.program, row.supplier);
            let delivered = dependency_citation(row.program, row.receiver);
            let route = MapRoute {
                use_site: Some(delivered.clone()),
                origin: RoutePoint::Project(ProjectPoint {
                    module: ModuleId(row.supplier.into()),
                    item: Some(row.carries.into()),
                }),
                hops: vec![
                    MapHop {
                        to: hop_target(row.program, HopLink::ReceiverUsesSender),
                        evidence: obtained.clone(),
                        support: support.clone(),
                        role: Some(row.first_role),
                    },
                    MapHop {
                        to: hop_target(row.receiver, HopLink::SenderUsesReceiver),
                        evidence: delivered.clone(),
                        support,
                        role: Some(UseRole::SuppliesInput),
                    },
                ],
                end: MapRouteEnd::ReachedUse,
                description: judged(
                    RouteDescription {
                        role: row.role,
                        other_roles: vec![],
                    },
                    row.note,
                    delivered,
                    vec![obtained],
                ),
            };
            (RouteId(row.key.into()), route)
        })
        .collect();

    ProjectMap {
        graph_id: GraphId("dep-mapper/proposal@e525f96".into()),
        project: "dep-mapper".into(),
        source_revision: SourceRevision("e525f96".into()),
        target: "proposed architecture".into(),
        configuration: "design proposal, not built".into(),
        layers: Layers::new(
            tables::LAYERS
                .iter()
                .map(|layer| LayerName(layer.to_string()))
                .collect(),
        )
        .expect("the example lists each layer once"),
        modules,
        dependencies,
        routes,
        evidence,
        coverage_gaps: vec![proposal_evidence(
            "No source was analyzed. Most modules exist only in the design.".into(),
        )],
    }
}

#[cfg(test)]
mod tests;
