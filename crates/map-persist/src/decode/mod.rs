//! Turns file text into a map or draft, checking what the format types
//! cannot express: which review fields go with which state, the mix of
//! fields in a point, roles, statuses, and duplicate entries.

use crate::format::*;
use crate::{LoadError, MapDocument, Position};
use project_map::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use toml::Spanned;

type Result<T> = std::result::Result<T, LoadError>;

pub fn document(text: &str) -> Result<MapDocument> {
    let decoder = Decoder { text };
    let header: Header = decoder.parse()?;
    if *header.format.get_ref() != VERSION {
        return Err(decoder.error(
            header.format.span(),
            format!(
                "format version {} is not supported; this program reads version {VERSION}",
                header.format.get_ref()
            ),
        ));
    }
    match header.kind.get_ref().as_str() {
        "map" => decoder.map(decoder.parse()?).map(MapDocument::Map),
        "draft" => decoder.draft(decoder.parse()?).map(MapDocument::Draft),
        other => Err(decoder.error(
            header.kind.span(),
            format!("kind {other:?} is not \"map\" or \"draft\""),
        )),
    }
}

/// How to read a part whose `status` is left out.
#[derive(Clone, Copy)]
enum MissingStatus {
    /// In a saved map, `status` is required.
    Refused,
    /// In a draft, it means proposed.
    Proposed,
}

/// Where a point appears, which decides the fields it may have.
#[derive(Clone, Copy)]
enum PointUse {
    Origin,
    Hop,
}

/// What a review is about, which decides the fields of a judgment.
#[derive(Clone, Copy)]
enum Subject {
    Module,
    Connection,
}

struct Decoder<'a> {
    text: &'a str,
}

impl Decoder<'_> {
    fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        toml::from_str(self.text).map_err(|error| LoadError {
            position: error.span().map(|span| self.position(span.start)),
            message: error.message().trim_end().to_string(),
        })
    }

    fn position(&self, offset: usize) -> Position {
        let before = &self.text[..offset.min(self.text.len())];
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        Position {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }

    fn error(&self, span: Range<usize>, message: String) -> LoadError {
        LoadError {
            position: Some(self.position(span.start)),
            message,
        }
    }

    fn map(&self, file: MapFile) -> Result<ProjectMap> {
        let missing = MissingStatus::Refused;
        Ok(ProjectMap {
            project: file.project,
            source: file.source.map(|source| MapSource {
                graph_id: GraphId(source.graph),
                source_revision: SourceRevision(source.revision),
                target: source.target,
                configuration: source.configuration,
            }),
            layers: self.layers(&file.layers)?,
            modules: self.modules(&file.modules, missing)?,
            dependencies: self.dependencies(&file.dependencies, missing)?,
            routes: self.routes(&file.routes, missing)?,
            evidence: self.evidence(&file.evidence)?,
            coverage_gaps: file
                .gaps
                .iter()
                .map(|gap| self.evidence_entry(gap))
                .collect::<Result<_>>()?,
        })
    }

    fn draft(&self, file: DraftFile) -> Result<Draft> {
        let base = match (file.base, file.project) {
            (Some(base), None) => DraftBase::Saved(base.into_inner()),
            (None, Some(project)) => DraftBase::None {
                project: project.into_inner(),
            },
            (Some(_), Some(project)) => {
                return Err(self.error(
                    project.span(),
                    "a draft with a base map takes the project's name from it; remove `project`"
                        .into(),
                ));
            }
            (None, None) => {
                return Err(LoadError {
                    position: None,
                    message: "a draft needs a `base` map or, standing alone, a `project` name"
                        .into(),
                });
            }
        };
        let remove = match &file.remove {
            Some(remove) => self.removals(remove)?,
            None => Removals::default(),
        };
        if let (DraftBase::None { .. }, Some(spanned)) = (&base, &file.remove)
            && remove != Removals::default()
        {
            return Err(self.error(
                spanned.span(),
                "a draft without a base map has nothing to remove".into(),
            ));
        }
        let missing = MissingStatus::Proposed;
        Ok(Draft {
            base,
            layers: match &file.layers {
                Some(layers) => self.layers(layers)?,
                None => Layers::empty(),
            },
            modules: self.modules(&file.modules, missing)?,
            dependencies: self.dependencies(&file.dependencies, missing)?,
            routes: self.routes(&file.routes, missing)?,
            evidence: self.evidence(&file.evidence)?,
            remove,
        })
    }

    fn layers(&self, layers: &Spanned<Vec<String>>) -> Result<Layers> {
        let names = layers.get_ref().iter().cloned().map(LayerName).collect();
        Layers::new(names).map_err(|duplicate| self.error(layers.span(), duplicate.to_string()))
    }

    fn status(
        &self,
        status: Option<StatusDoc>,
        missing: MissingStatus,
        span: Range<usize>,
    ) -> Result<Status> {
        match (status, missing) {
            (Some(StatusDoc::Observed), _) => Ok(Status::Observed),
            (Some(StatusDoc::Proposed), _) | (None, MissingStatus::Proposed) => {
                Ok(Status::Proposed)
            }
            (Some(StatusDoc::ProposedRemoval), _) => Ok(Status::ProposedRemoval),
            (None, MissingStatus::Refused) => {
                Err(self.error(span, "`status` is required in a saved map".into()))
            }
        }
    }

    fn modules(
        &self,
        modules: &BTreeMap<String, Spanned<ModuleDoc>>,
        missing: MissingStatus,
    ) -> Result<BTreeMap<ModuleId, MapModule>> {
        modules
            .iter()
            .map(|(key, module)| {
                let doc = module.get_ref();
                let surface = doc
                    .surface
                    .iter()
                    .map(|item| {
                        Ok(SurfaceItem {
                            name: item.name.clone(),
                            kind: item_kind(item.kind),
                            signature: item.signature.clone(),
                            declaration: item
                                .declaration
                                .as_ref()
                                .map(|location| self.location(location))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_>>()?;
                let value = MapModule {
                    status: self.status(doc.status, missing, module.span())?,
                    name: doc.name.clone(),
                    surface,
                    description: self.review(&doc.review, Subject::Module, |review| {
                        Ok(ModuleDescription {
                            concern: self.required(&doc.review, &review.concern, "concern")?,
                            kind: module_kind(self.required(&doc.review, &review.kind, "kind")?),
                            layer: review.layer.clone().map(LayerName),
                        })
                    })?,
                };
                Ok((ModuleId(key.clone()), value))
            })
            .collect()
    }

    fn dependencies(
        &self,
        dependencies: &[Spanned<DependencyDoc>],
        missing: MissingStatus,
    ) -> Result<BTreeMap<DependencyId, MapDependency>> {
        let mut decoded = BTreeMap::new();
        for dependency in dependencies {
            let doc = dependency.get_ref();
            let id = DependencyId {
                from: ModuleId(doc.from.clone()),
                to: ModuleId(doc.to.clone()),
            };
            let value = MapDependency {
                status: self.status(doc.status, missing, dependency.span())?,
                evidence: doc.evidence.iter().cloned().map(CitationId).collect(),
                description: self.connection_review(&doc.review)?,
            };
            if decoded.insert(id, value).is_some() {
                return Err(self.error(
                    dependency.span(),
                    format!(
                        "the dependency of {} on {} is listed twice",
                        doc.from, doc.to
                    ),
                ));
            }
        }
        Ok(decoded)
    }

    fn routes(
        &self,
        routes: &BTreeMap<String, Spanned<RouteDoc>>,
        missing: MissingStatus,
    ) -> Result<BTreeMap<RouteId, MapRoute>> {
        routes
            .iter()
            .map(|(key, route)| {
                let doc = route.get_ref();
                let RoutePointOrTarget::Origin(origin) =
                    self.point(&doc.origin, PointUse::Origin)?
                else {
                    unreachable!("an origin decodes to a route point")
                };
                let hops = doc
                    .hops
                    .iter()
                    .map(|hop| self.hop(hop))
                    .collect::<Result<_>>()?;
                let value = MapRoute {
                    status: self.status(doc.status, missing, route.span())?,
                    use_site: doc.use_site.clone().map(CitationId),
                    origin,
                    hops,
                    end: self.end(&doc.end)?,
                    description: self.connection_review(&doc.review)?,
                };
                Ok((RouteId(key.clone()), value))
            })
            .collect()
    }

    fn hop(&self, hop: &HopDoc) -> Result<MapHop> {
        let RoutePointOrTarget::Target(to) = self.point(&hop.to, PointUse::Hop)? else {
            unreachable!("a hop decodes to a hop target")
        };
        let support = match (*hop.support.get_ref(), &hop.condition) {
            (SupportDoc::Established, None) => HopSupport::Established,
            (SupportDoc::Proposed, None) => HopSupport::Proposed,
            (SupportDoc::Possible, condition) => HopSupport::Possible {
                condition: condition.clone(),
            },
            (SupportDoc::Established | SupportDoc::Proposed, Some(_)) => {
                return Err(self.error(
                    hop.support.span(),
                    "only a possible hop has a `condition`".into(),
                ));
            }
        };
        Ok(MapHop {
            to,
            evidence: CitationId(hop.evidence.clone()),
            support,
            role: hop.role.as_ref().map(|role| self.role(role)).transpose()?,
        })
    }

    fn point(&self, point: &Spanned<PointDoc>, place: PointUse) -> Result<RoutePointOrTarget> {
        let doc = point.get_ref();
        let fail = |message: &str| Err(self.error(point.span(), message.into()));
        let outside = match (&doc.module, &doc.external, &doc.unresolved) {
            (Some(_), None, None) => None,
            (None, Some(name), None) => Some(OutsidePoint::External(name.clone())),
            (None, None, Some(name)) => Some(OutsidePoint::Unresolved(name.clone())),
            _ => return fail("a point has exactly one of `module`, `external` or `unresolved`"),
        };
        if let Some(outside) = outside {
            if doc.item.is_some() || doc.link.is_some() {
                return fail("a point outside the project has no `item` or `link`");
            }
            return Ok(match place {
                PointUse::Origin => RoutePointOrTarget::Origin(RoutePoint::Outside(outside)),
                PointUse::Hop => RoutePointOrTarget::Target(HopTarget::Outside(outside)),
            });
        }
        let project = ProjectPoint {
            module: ModuleId(doc.module.clone().unwrap_or_default()),
            item: doc.item.clone(),
        };
        match (place, doc.link) {
            (PointUse::Origin, None) => {
                Ok(RoutePointOrTarget::Origin(RoutePoint::Project(project)))
            }
            (PointUse::Origin, Some(_)) => fail("a route's origin has no `link`"),
            (PointUse::Hop, Some(link)) => Ok(RoutePointOrTarget::Target(HopTarget::Project {
                point: project,
                link: hop_link(link),
            })),
            (PointUse::Hop, None) => fail("a hop into a project module needs a `link`"),
        }
    }

    fn end(&self, end: &Spanned<EndDoc>) -> Result<MapRouteEnd> {
        let doc = end.get_ref();
        match (doc.state, &doc.at, &doc.reason) {
            (EndStateDoc::Reached, None, None) => Ok(MapRouteEnd::ReachedUse),
            (EndStateDoc::Stopped, Some(at), Some(reason)) => Ok(MapRouteEnd::Stopped {
                at: CitationId(at.clone()),
                reason: reason.clone(),
            }),
            (EndStateDoc::Reached, _, _) => Err(self.error(
                end.span(),
                "a route that reached its use has no `at` or `reason`".into(),
            )),
            (EndStateDoc::Stopped, _, _) => {
                Err(self.error(end.span(), "a stopped route needs `at` and `reason`".into()))
            }
        }
    }

    fn connection_review(&self, review: &Spanned<ReviewDoc>) -> Result<Review<RouteDescription>> {
        self.review(review, Subject::Connection, |doc| {
            Ok(RouteDescription {
                role: self.role(&self.required(review, &doc.role, "role")?)?,
                other_roles: doc
                    .other_roles
                    .iter()
                    .flatten()
                    .map(|role| self.role(role))
                    .collect::<Result<_>>()?,
            })
        })
    }

    /// Checks the fields against the review's state, then reads a judgment
    /// with `judgment`.
    fn review<T>(
        &self,
        review: &Spanned<ReviewDoc>,
        subject: Subject,
        judgment: impl FnOnce(&ReviewDoc) -> Result<T>,
    ) -> Result<Review<T>> {
        let doc = review.get_ref();
        let present = [
            ("reason", doc.reason.is_some()),
            ("concern", doc.concern.is_some()),
            ("kind", doc.kind.is_some()),
            ("layer", doc.layer.is_some()),
            ("role", doc.role.is_some()),
            ("other_roles", doc.other_roles.is_some()),
            ("primary", doc.primary.is_some()),
            ("supporting", doc.supporting.is_some()),
        ];
        let (state, allowed): (&str, &[&str]) = match (doc.state, subject) {
            (ReviewStateDoc::Unreviewed, _) => ("an unreviewed", &[]),
            (ReviewStateDoc::Deferred, _) => ("a deferred", &["reason"]),
            (ReviewStateDoc::Judged, Subject::Module) => (
                "a judged module",
                &[
                    "reason",
                    "concern",
                    "kind",
                    "layer",
                    "primary",
                    "supporting",
                ],
            ),
            (ReviewStateDoc::Judged, Subject::Connection) => (
                "a judged",
                &["reason", "role", "other_roles", "primary", "supporting"],
            ),
        };
        if let Some((field, _)) = present
            .iter()
            .find(|(field, set)| *set && !allowed.contains(field))
        {
            return Err(self.error(review.span(), format!("{state} review has no `{field}`")));
        }
        Ok(match doc.state {
            ReviewStateDoc::Unreviewed => Review::Unreviewed,
            ReviewStateDoc::Deferred => Review::Deferred {
                reason: self.required(review, &doc.reason, "reason")?,
            },
            ReviewStateDoc::Judged => Review::Judged {
                judgment: judgment(doc)?,
                reason: self.required(review, &doc.reason, "reason")?,
                primary: CitationId(self.required(review, &doc.primary, "primary")?),
                supporting: doc
                    .supporting
                    .iter()
                    .flatten()
                    .cloned()
                    .map(CitationId)
                    .collect(),
            },
        })
    }

    fn required<T: Clone>(
        &self,
        review: &Spanned<ReviewDoc>,
        field: &Option<T>,
        name: &str,
    ) -> Result<T> {
        field.clone().ok_or_else(|| {
            let state = match review.get_ref().state {
                ReviewStateDoc::Deferred => "a deferred",
                _ => "a judged",
            };
            self.error(review.span(), format!("{state} review needs `{name}`"))
        })
    }

    fn role(&self, role: &Spanned<String>) -> Result<UseRole> {
        let text = role.get_ref();
        if let Some(other) = text.strip_prefix("other:") {
            let other = other.trim();
            if other.is_empty() {
                return Err(self.error(role.span(), "`other:` needs a description".into()));
            }
            return Ok(UseRole::Other(other.into()));
        }
        ROLES
            .iter()
            .find(|(slug, _)| slug == text)
            .map(|(_, role)| role.clone())
            .ok_or_else(|| {
                let names: Vec<&str> = ROLES.iter().map(|(slug, _)| *slug).collect();
                self.error(
                    role.span(),
                    format!(
                        "unknown role {text:?}; use one of {}, or \"other: <description>\"",
                        names.join(", ")
                    ),
                )
            })
    }

    fn evidence(
        &self,
        evidence: &BTreeMap<String, EvidenceDoc>,
    ) -> Result<BTreeMap<CitationId, MapEvidence>> {
        evidence
            .iter()
            .map(|(key, entry)| Ok((CitationId(key.clone()), self.evidence_entry(entry)?)))
            .collect()
    }

    fn evidence_entry(&self, entry: &EvidenceDoc) -> Result<MapEvidence> {
        Ok(MapEvidence {
            kind: match entry.kind {
                EvidenceKindDoc::Source => EvidenceKind::Source,
                EvidenceKindDoc::Proposal => EvidenceKind::Proposal,
            },
            build_unit: entry.unit.clone().map(UnitId),
            summary: entry.summary.clone(),
            location: entry
                .location
                .as_ref()
                .map(|location| self.location(location))
                .transpose()?,
        })
    }

    fn location(&self, location: &Spanned<LocationDoc>) -> Result<Location> {
        let doc = location.get_ref();
        if doc.start > doc.end {
            return Err(self.error(
                location.span(),
                format!("`start` ({}) is after `end` ({})", doc.start, doc.end),
            ));
        }
        Ok(Location {
            file: SourcePath(doc.file.clone()),
            bytes: doc.start..doc.end,
        })
    }

    fn removals(&self, remove: &Spanned<RemoveDoc>) -> Result<Removals> {
        let doc = remove.get_ref();
        Ok(Removals {
            modules: self.unique(&doc.modules, |name| ModuleId(name.clone()))?,
            dependencies: self.unique(&doc.dependencies, |key| DependencyId {
                from: ModuleId(key.from.clone()),
                to: ModuleId(key.to.clone()),
            })?,
            routes: self.unique(&doc.routes, |name| RouteId(name.clone()))?,
        })
    }

    fn unique<D, T: Ord>(
        &self,
        items: &[Spanned<D>],
        read: impl Fn(&D) -> T,
    ) -> Result<BTreeSet<T>> {
        let mut set = BTreeSet::new();
        for item in items {
            if !set.insert(read(item.get_ref())) {
                return Err(self.error(item.span(), "listed twice for removal".into()));
            }
        }
        Ok(set)
    }
}

enum RoutePointOrTarget {
    Origin(RoutePoint),
    Target(HopTarget),
}

/// Each role's name in the file, shared with encoding.
pub(crate) const ROLES: [(&str, UseRole); 7] = [
    ("direct-use", UseRole::DirectUse),
    ("implements", UseRole::Implements),
    ("wires", UseRole::Wires),
    ("supplies-input", UseRole::SuppliesInput),
    ("full-injection", UseRole::FullInjection),
    ("adapted-injection", UseRole::AdaptedInjection),
    ("incidental", UseRole::Incidental),
];

fn item_kind(kind: ItemKindDoc) -> ItemKind {
    match kind {
        ItemKindDoc::Type => ItemKind::Type,
        ItemKindDoc::Value => ItemKind::Value,
        ItemKindDoc::Function => ItemKind::Function,
        ItemKindDoc::Method => ItemKind::Method,
        ItemKindDoc::Module => ItemKind::Module,
    }
}

fn module_kind(kind: ModuleKindDoc) -> ModuleKind {
    match kind {
        ModuleKindDoc::Logic => ModuleKind::Logic,
        ModuleKindDoc::Effect => ModuleKind::Effect,
        ModuleKindDoc::Composition => ModuleKind::Composition,
        ModuleKindDoc::Contract => ModuleKind::Contract,
    }
}

fn hop_link(link: LinkDoc) -> HopLink {
    match link {
        LinkDoc::ReceiverUsesSender => HopLink::ReceiverUsesSender,
        LinkDoc::SenderUsesReceiver => HopLink::SenderUsesReceiver,
        LinkDoc::SameModule => HopLink::SameModule,
        LinkDoc::FromOutside => HopLink::FromOutside,
    }
}
