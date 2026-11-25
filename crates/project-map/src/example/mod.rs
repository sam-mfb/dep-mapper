//! sharsheret's own proposed architecture as a `ProjectMap`, for consumers
//! that need a realistic map to display or store. This is the project's
//! module map: update it as the architecture changes, and view it with
//! `cargo run -p map-web --example preview`.
//! No source was analyzed, so evidence entries describe the proposal and
//! carry no source locations. Every part is marked observed except one
//! proposed route, `PROPOSED_ROUTE`, until the map becomes a draft file.

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

/// A route from a supplier to the program, and on to a receiver when the
/// program passes the value along.
struct RouteRow {
    key: &'static str,
    supplier: &'static str,
    /// The supplier's item that travels along the route.
    carries: &'static str,
    program: &'static str,
    /// The role of the program's use of the supplier.
    first_role: UseRole,
    /// The module the program passes the value into; `None` when the
    /// program uses the value itself.
    receiver: Option<&'static str>,
    role: UseRole,
    /// Present when the route applies only under this condition.
    condition: Option<&'static str>,
    note: &'static str,
}

/// The one route marked proposed, so consumers have a proposal to show.
pub const PROPOSED_ROUTE: &str = "present-the-project-map";

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
        kind: EvidenceKind::Source,
        build_unit: None,
        summary,
        location: None,
    }
}

/// The sharsheret module map. Every route reaches its use; the TypeScript
/// grapher's hops are possible only for a TypeScript project.
pub fn sharsheret() -> ProjectMap {
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
                status: Status::Observed,
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
                status: Status::Observed,
                evidence: vec![citation.clone()],
                description: judged(description, row.detail, citation, vec![]),
            };
            (id, dependency)
        })
        .collect();

    let routes = tables::routes()
        .into_iter()
        .map(|row| {
            let proposed = row.key == PROPOSED_ROUTE;
            let support = match (proposed, row.condition) {
                (true, _) => HopSupport::Proposed,
                (false, Some(condition)) => HopSupport::Possible {
                    condition: Some(condition.into()),
                },
                (false, None) => HopSupport::Established,
            };
            let obtained = dependency_citation(row.program, row.supplier);
            let mut hops = vec![MapHop {
                to: hop_target(row.program, HopLink::ReceiverUsesSender),
                evidence: obtained.clone(),
                support: support.clone(),
                role: Some(row.first_role),
            }];
            // The route reaches its use where the value is delivered, or where
            // the program uses it when it keeps the value.
            let (used, supporting) = match row.receiver {
                Some(receiver) => {
                    let delivered = dependency_citation(row.program, receiver);
                    hops.push(MapHop {
                        to: hop_target(receiver, HopLink::SenderUsesReceiver),
                        evidence: delivered.clone(),
                        support,
                        role: Some(UseRole::SuppliesInput),
                    });
                    (delivered, vec![obtained])
                }
                None => (obtained, vec![]),
            };
            // A proposed route rests its judgment on a proposal note.
            let (status, primary, supporting) = if proposed {
                let note = CitationId(format!("proposal:{}", row.key));
                evidence.insert(
                    note.clone(),
                    MapEvidence {
                        kind: EvidenceKind::Proposal,
                        build_unit: None,
                        summary: row.note.into(),
                        location: None,
                    },
                );
                let supporting = std::iter::once(used.clone()).chain(supporting).collect();
                (Status::ProposedAddition, note, supporting)
            } else {
                (Status::Observed, used.clone(), supporting)
            };
            let route = MapRoute {
                status,
                use_site: Some(used.clone()),
                origin: RoutePoint::Project(ProjectPoint {
                    module: ModuleId(row.supplier.into()),
                    item: Some(row.carries.into()),
                }),
                hops,
                end: MapRouteEnd::ReachedUse,
                description: judged(
                    RouteDescription {
                        role: row.role,
                        other_roles: vec![],
                    },
                    row.note,
                    primary,
                    supporting,
                ),
            };
            (RouteId(row.key.into()), route)
        })
        .collect();

    ProjectMap {
        project: "sharsheret".into(),
        source: Some(MapSource {
            graph_id: GraphId("sharsheret/proposal@e525f96".into()),
            source_revision: SourceRevision("e525f96".into()),
            target: "proposed architecture".into(),
            configuration: "design proposal, not built".into(),
        }),
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
