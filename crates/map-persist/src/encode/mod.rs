//! Turns a map or draft into file text. Every part's `status` is written,
//! even in a draft, so the text says what it means without the defaults.

use crate::MapDocument;
use crate::decode::ROLES;
use crate::format::*;
use project_map::*;
use std::collections::BTreeMap;
use toml::Spanned;

pub fn document(document: &MapDocument) -> String {
    let text = match document {
        MapDocument::Map(map) => toml::to_string(&map_file(map)),
        MapDocument::Draft(draft) => toml::to_string(&draft_file(draft)),
    };
    text.expect("the format types use only TOML-compatible shapes")
}

/// A value in the file, with no place in any text.
fn unplaced<T>(value: T) -> Spanned<T> {
    Spanned::new(0..0, value)
}

fn map_file(map: &ProjectMap) -> MapFile {
    MapFile {
        format: VERSION,
        kind: "map".into(),
        project: map.project.clone(),
        source: map.source.as_ref().map(|source| SourceDoc {
            graph: source.graph_id.0.clone(),
            revision: source.source_revision.0.clone(),
            target: source.target.clone(),
            configuration: source.configuration.clone(),
        }),
        layers: unplaced(layer_names(&map.layers)),
        modules: modules(&map.modules),
        dependencies: dependencies(&map.dependencies),
        routes: routes(&map.routes),
        evidence: evidence(&map.evidence),
        gaps: map.coverage_gaps.iter().map(evidence_entry).collect(),
    }
}

fn draft_file(draft: &Draft) -> DraftFile {
    let (base, project) = match &draft.base {
        DraftBase::Saved(base) => (Some(unplaced(base.clone())), None),
        DraftBase::None { project } => (None, Some(unplaced(project.clone()))),
    };
    let remove = &draft.remove;
    DraftFile {
        format: VERSION,
        kind: "draft".into(),
        base,
        project,
        layers: Some(unplaced(layer_names(&draft.layers))),
        modules: modules(&draft.modules),
        dependencies: dependencies(&draft.dependencies),
        routes: routes(&draft.routes),
        evidence: evidence(&draft.evidence),
        update: Some(unplaced(updates(&draft.update))),
        remove: Some(unplaced(RemoveDoc {
            modules: remove
                .modules
                .iter()
                .map(|m| unplaced(m.0.clone()))
                .collect(),
            dependencies: remove
                .dependencies
                .iter()
                .map(|id| {
                    unplaced(DependencyKeyDoc {
                        from: id.from.0.clone(),
                        to: id.to.0.clone(),
                    })
                })
                .collect(),
            routes: remove
                .routes
                .iter()
                .map(|r| unplaced(r.0.clone()))
                .collect(),
        })),
    }
}

fn layer_names(layers: &Layers) -> Vec<String> {
    layers.iter().map(|layer| layer.0.clone()).collect()
}

/// The status, and the review a proposed update carries, written by `review`.
fn status<D>(
    status: &Status<D>,
    review: impl Fn(&Review<D>) -> Spanned<ReviewDoc>,
) -> (Option<StatusDoc>, Option<Spanned<ReviewDoc>>) {
    let (status, proposed) = match status {
        Status::Observed => (StatusDoc::Observed, None),
        Status::ProposedAddition => (StatusDoc::ProposedAddition, None),
        Status::ProposedUpdate { proposed } => (StatusDoc::ProposedUpdate, Some(review(proposed))),
        Status::ProposedRemoval => (StatusDoc::ProposedRemoval, None),
    };
    (Some(status), proposed)
}

fn updates(updates: &Updates) -> UpdateDoc {
    UpdateDoc {
        modules: updates
            .modules
            .iter()
            .map(|(id, update)| {
                let changes = &update.changes;
                let doc = ChangeDoc {
                    concern: changes.concern.clone(),
                    kind: changes.kind.as_ref().map(module_kind),
                    layer: changes.layer.as_ref().map(|layer| layer.0.clone()),
                    role: None,
                    other_roles: None,
                    reason: update.reason.clone(),
                    primary: update.primary.0.clone(),
                    supporting: citations(&update.supporting),
                };
                (id.0.clone(), unplaced(doc))
            })
            .collect(),
        dependencies: updates
            .dependencies
            .iter()
            .map(|(id, update)| {
                let (role, other_roles) = role_changes(&update.changes);
                unplaced(DependencyChangeDoc {
                    from: id.from.0.clone(),
                    to: id.to.0.clone(),
                    role,
                    other_roles,
                    reason: update.reason.clone(),
                    primary: update.primary.0.clone(),
                    supporting: citations(&update.supporting),
                })
            })
            .collect(),
        routes: updates
            .routes
            .iter()
            .map(|(id, update)| {
                let (role, other_roles) = role_changes(&update.changes);
                let doc = ChangeDoc {
                    concern: None,
                    kind: None,
                    layer: None,
                    role,
                    other_roles,
                    reason: update.reason.clone(),
                    primary: update.primary.0.clone(),
                    supporting: citations(&update.supporting),
                };
                (id.0.clone(), unplaced(doc))
            })
            .collect(),
    }
}

type RoleDocs = (Option<Spanned<String>>, Option<Vec<Spanned<String>>>);

fn role_changes(changes: &ConnectionChanges) -> RoleDocs {
    (
        changes.role.as_ref().map(|role| unplaced(role_name(role))),
        changes
            .other_roles
            .as_ref()
            .map(|roles| roles.iter().map(|role| unplaced(role_name(role))).collect()),
    )
}

fn citations(citations: &[CitationId]) -> Vec<String> {
    citations.iter().map(|c| c.0.clone()).collect()
}

fn modules(modules: &BTreeMap<ModuleId, MapModule>) -> BTreeMap<String, Spanned<ModuleDoc>> {
    modules
        .iter()
        .map(|(id, module)| {
            let (status, proposed) = status(&module.status, module_review);
            let doc = ModuleDoc {
                status,
                name: module.name.clone(),
                surface: module
                    .surface
                    .iter()
                    .map(|item| SurfaceDoc {
                        name: item.name.clone(),
                        kind: item_kind(&item.kind),
                        signature: item.signature.clone(),
                        declaration: item.declaration.as_ref().map(location),
                        used_by: Some(unplaced(
                            item.used_by
                                .iter()
                                .map(|user| unplaced(user.0.clone()))
                                .collect(),
                        )),
                    })
                    .collect(),
                review: module_review(&module.description),
                proposed,
            };
            (id.0.clone(), unplaced(doc))
        })
        .collect()
}

fn dependencies(
    dependencies: &BTreeMap<DependencyId, MapDependency>,
) -> Vec<Spanned<DependencyDoc>> {
    dependencies
        .iter()
        .map(|(id, dependency)| {
            let (status, proposed) = status(&dependency.status, connection_review);
            unplaced(DependencyDoc {
                from: id.from.0.clone(),
                to: id.to.0.clone(),
                status,
                evidence: citations(&dependency.evidence),
                review: connection_review(&dependency.description),
                proposed,
            })
        })
        .collect()
}

fn routes(routes: &BTreeMap<RouteId, MapRoute>) -> BTreeMap<String, Spanned<RouteDoc>> {
    routes
        .iter()
        .map(|(id, route)| {
            let (status, proposed) = status(&route.status, connection_review);
            let doc = RouteDoc {
                status,
                use_site: None,
                use_sites: Some(unplaced(citations(&route.use_sites))),
                origin: unplaced(match &route.origin {
                    RoutePoint::Project(point) => project_point(point),
                    RoutePoint::Outside(point) => outside_point(point),
                }),
                hops: route.hops.iter().map(hop).collect(),
                end: unplaced(match &route.end {
                    MapRouteEnd::ReachedUse => EndDoc {
                        state: EndStateDoc::Reached,
                        at: None,
                        reason: None,
                    },
                    MapRouteEnd::Stopped { at, reason } => EndDoc {
                        state: EndStateDoc::Stopped,
                        at: Some(Citations::List(citations(at))),
                        reason: Some(reason.clone()),
                    },
                }),
                review: connection_review(&route.description),
                proposed,
            };
            (id.0.clone(), unplaced(doc))
        })
        .collect()
}

fn hop(hop: &MapHop) -> HopDoc {
    let to = match &hop.to {
        HopTarget::Project { point, link } => PointDoc {
            link: Some(hop_link(link)),
            ..project_point(point)
        },
        HopTarget::Outside(point) => outside_point(point),
    };
    let (support, condition) = match &hop.support {
        HopSupport::Established => (SupportDoc::Established, None),
        HopSupport::Possible { condition } => (SupportDoc::Possible, condition.clone()),
        HopSupport::Proposed => (SupportDoc::Proposed, None),
    };
    HopDoc {
        to: unplaced(to),
        evidence: unplaced(Citations::List(citations(&hop.evidence))),
        support: unplaced(support),
        condition,
        role: hop.role.as_ref().map(|role| unplaced(role_name(role))),
    }
}

fn project_point(point: &ProjectPoint) -> PointDoc {
    PointDoc {
        module: Some(point.module.0.clone()),
        item: point.item.clone(),
        link: None,
        external: None,
        unresolved: None,
    }
}

fn outside_point(point: &OutsidePoint) -> PointDoc {
    let (external, unresolved) = match point {
        OutsidePoint::External(name) => (Some(name.clone()), None),
        OutsidePoint::Unresolved(name) => (None, Some(name.clone())),
    };
    PointDoc {
        module: None,
        item: None,
        link: None,
        external,
        unresolved,
    }
}

fn module_review(description: &Review<ModuleDescription>) -> Spanned<ReviewDoc> {
    review(description, |judgment, doc| {
        doc.concern = Some(judgment.concern.clone());
        doc.kind = Some(module_kind(&judgment.kind));
        doc.layer = judgment.layer.as_ref().map(|layer| layer.0.clone());
    })
}

fn connection_review(description: &Review<RouteDescription>) -> Spanned<ReviewDoc> {
    review(description, |judgment, doc| {
        doc.role = Some(unplaced(role_name(&judgment.role)));
        doc.other_roles = Some(
            judgment
                .other_roles
                .iter()
                .map(|role| unplaced(role_name(role)))
                .collect(),
        );
    })
}

/// Writes the review's state and common fields; `judgment` adds the
/// judgment's own fields.
fn review<T>(review: &Review<T>, judgment: impl FnOnce(&T, &mut ReviewDoc)) -> Spanned<ReviewDoc> {
    let mut doc = ReviewDoc {
        state: ReviewStateDoc::Unreviewed,
        reason: None,
        concern: None,
        kind: None,
        layer: None,
        role: None,
        other_roles: None,
        primary: None,
        supporting: None,
    };
    match review {
        Review::Unreviewed => {}
        Review::Deferred { reason } => {
            doc.state = ReviewStateDoc::Deferred;
            doc.reason = Some(reason.clone());
        }
        Review::Judged {
            judgment: value,
            reason,
            primary,
            supporting,
        } => {
            doc.state = ReviewStateDoc::Judged;
            doc.reason = Some(reason.clone());
            doc.primary = Some(primary.0.clone());
            doc.supporting = Some(supporting.iter().map(|c| c.0.clone()).collect());
            judgment(value, &mut doc);
        }
    }
    unplaced(doc)
}

fn role_name(role: &UseRole) -> String {
    match role {
        UseRole::Other(other) => format!("other: {other}"),
        named => ROLES
            .iter()
            .find(|(_, role)| role == named)
            .map(|(slug, _)| slug.to_string())
            .expect("every named role has a file name"),
    }
}

fn evidence(evidence: &BTreeMap<CitationId, MapEvidence>) -> BTreeMap<String, EvidenceDoc> {
    evidence
        .iter()
        .map(|(id, entry)| (id.0.clone(), evidence_entry(entry)))
        .collect()
}

fn evidence_entry(entry: &MapEvidence) -> EvidenceDoc {
    EvidenceDoc {
        kind: match entry.kind {
            EvidenceKind::Source => EvidenceKindDoc::Source,
            EvidenceKind::Proposal => EvidenceKindDoc::Proposal,
        },
        summary: entry.summary.clone(),
        unit: entry.build_unit.as_ref().map(|unit| unit.0.clone()),
        location: entry.location.as_ref().map(location),
    }
}

fn location(location: &Location) -> Spanned<LocationDoc> {
    unplaced(LocationDoc {
        file: location.file.0.clone(),
        start: location.bytes.start,
        end: location.bytes.end,
    })
}

fn item_kind(kind: &ItemKind) -> ItemKindDoc {
    match kind {
        ItemKind::Type => ItemKindDoc::Type,
        ItemKind::Value => ItemKindDoc::Value,
        ItemKind::Function => ItemKindDoc::Function,
        ItemKind::Method => ItemKindDoc::Method,
        ItemKind::Module => ItemKindDoc::Module,
    }
}

fn module_kind(kind: &ModuleKind) -> ModuleKindDoc {
    match kind {
        ModuleKind::Logic => ModuleKindDoc::Logic,
        ModuleKind::Effect => ModuleKindDoc::Effect,
        ModuleKind::Composition => ModuleKindDoc::Composition,
        ModuleKind::Contract => ModuleKindDoc::Contract,
    }
}

fn hop_link(link: &HopLink) -> LinkDoc {
    match link {
        HopLink::ReceiverUsesSender => LinkDoc::ReceiverUsesSender,
        HopLink::SenderUsesReceiver => LinkDoc::SenderUsesReceiver,
        HopLink::SameModule => LinkDoc::SameModule,
        HopLink::FromOutside => LinkDoc::FromOutside,
    }
}
