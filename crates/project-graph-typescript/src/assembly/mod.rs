//! Builds the `ProjectGraph` from the collector's records. Pure: the records,
//! the selected build and the trace limits are all inputs.
//!
//! Each workspace package is a module with one build unit for the selected
//! tsconfig. Facts belong to the unit of the package whose source holds them.
//!
//! The graph's ID ends with a fingerprint of everything else in the graph,
//! so two graphs with the same ID hold the same evidence, even when they
//! come from uncommitted source.

use crate::collection::records::{self, Record};
use crate::routes::{self, MoveLink, ReceiverUse, TraceLimits, ValueMoves};
use project_graph::{
    Build, BuildUnit, CoverageGap, Dispatch, Endpoint, Fact, FactId, FactKind, GraphId, ImportUse,
    Item, ItemId, ItemKind, Module, ModuleId, ProjectGraph, SourceSpan, UnitId, Visibility,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// What the grapher knows about the build before reading the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub source_revision: String,
    /// The tsconfig path, relative to the project root.
    pub target: String,
    pub configuration: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssemblyError {
    #[error("the collector output ended before its end record")]
    Incomplete,
    #[error("the collector output has no project record")]
    NoProject,
    #[error("a record names module {0}, which the collector did not list")]
    UnknownModule(String),
}

/// Assembles the graph, merges routes that follow the same path as
/// `route-merging` describes, then leaves out the routes that start in the
/// `excluded` packages as `route-exclusion` describes. An outside name lies in
/// a package when the package is the part before its `#`, as `clock` in
/// `clock#now`. The graph's ID covers what is left.
pub fn assemble(
    selected: &Selected,
    records: Vec<Record>,
    limits: TraceLimits,
    excluded: &[String],
) -> Result<ProjectGraph, AssemblyError> {
    if records.last() != Some(&Record::End) {
        return Err(AssemblyError::Incomplete);
    }
    let mut project = None;
    let mut modules = BTreeMap::new();
    let mut items = BTreeMap::new();
    let mut parents = BTreeMap::new();
    let mut facts = BTreeMap::new();
    let mut conditions = BTreeMap::new();
    let mut links = BTreeMap::new();
    let mut joining = BTreeSet::new();
    let mut receivers = Vec::new();
    let mut gaps = Vec::new();
    let unit_of = |module: &str| UnitId(format!("{module}:{}", selected.target));

    for record in records {
        match record {
            Record::Project { name, settings, .. } => project = Some((name, settings)),
            Record::Module { id, name, root } => {
                modules.insert(ModuleId(id), Module { name, root });
            }
            Record::Item {
                id,
                module,
                name,
                kind,
                parent,
                visibility,
                signature,
                declaration,
            } => {
                let module = known(&modules, module)?;
                if let Some(parent) = parent {
                    parents.insert(ItemId(id.clone()), ItemId(parent));
                }
                items.insert(
                    ItemId(id),
                    Item {
                        module,
                        name,
                        kind: item_kind(kind),
                        visibility: visibility_of(visibility),
                        signature,
                        declaration: declaration.map(span),
                    },
                );
            }
            Record::Fact {
                id,
                module,
                subject,
                fact,
                object,
                site,
                condition,
                link,
                joins_instances,
            } => {
                let module = known(&modules, module)?;
                let id = FactId(id);
                if let Some(condition) = condition {
                    conditions.insert(id.clone(), condition);
                }
                if joins_instances {
                    joining.insert(id.clone());
                }
                if let Some(link) = link {
                    let link = MoveLink {
                        argument: link.argument,
                        returned: link.returned,
                    };
                    links.insert(id.clone(), link);
                }
                facts.insert(
                    id,
                    Fact {
                        unit: unit_of(&module.0),
                        subject: endpoint(subject),
                        kind: fact_kind(fact),
                        object: endpoint(object),
                        site: site.map(span),
                    },
                );
            }
            Record::Receiver { call, moves } => receivers.push(ReceiverUse {
                call: FactId(call),
                moves: moves.into_iter().map(FactId).collect(),
            }),
            Record::Gap {
                module,
                site,
                reason,
            } => {
                let unit = match module {
                    Some(module) => Some(unit_of(&known(&modules, module)?.0)),
                    None => None,
                };
                gaps.push(CoverageGap {
                    unit,
                    site: site.map(span),
                    reason,
                });
            }
            Record::End => {}
        }
    }
    let (name, settings) = project.ok_or(AssemblyError::NoProject)?;

    let module_of = |endpoint: &Endpoint| match endpoint {
        Endpoint::Module(module) => Some(module.clone()),
        Endpoint::Item(item) => items.get(item).map(|i: &Item| i.module.clone()),
        Endpoint::Place { owner, .. } => items.get(owner).map(|i| i.module.clone()),
        Endpoint::External(_) | Endpoint::Unresolved(_) => None,
    };
    let mut routes = routes::direct_routes(&facts, module_of);
    // A field stands for every instance of its type, and a captured variable
    // for every instance of its closure.
    let captured: Vec<&Endpoint> = facts
        .values()
        .filter(|f| f.kind == FactKind::Capture)
        .map(|f| &f.object)
        .collect();
    let joins_instances = |endpoint: &Endpoint| match endpoint {
        Endpoint::Place { owner, projection } => {
            (projection != "this"
                && items
                    .get(owner)
                    .is_some_and(|i: &Item| i.kind == ItemKind::Type))
                || captured.contains(&endpoint)
        }
        _ => false,
    };
    let moves = ValueMoves {
        facts: &facts,
        links: &links,
        conditions: &conditions,
        joining: &joining,
    };
    routes.extend(routes::trace_values(
        &moves,
        &receivers,
        joins_instances,
        limits,
    ));
    facts.extend(routes::receiver_bounds(&facts, |method| {
        parents.get(method).cloned()
    }));

    let units = modules
        .keys()
        .map(|module| {
            let unit = BuildUnit {
                module: module.clone(),
                target: selected.target.clone(),
                settings: settings.clone(),
            };
            (unit_of(&module.0), unit)
        })
        .collect();
    let mut graph = ProjectGraph {
        id: GraphId(String::new()),
        build: Build {
            project: name.clone(),
            source_revision: selected.source_revision.clone(),
            target: selected.target.clone(),
            configuration: selected.configuration.clone(),
        },
        modules,
        units,
        items,
        facts,
        routes: route_merging::merge(routes),
        gaps,
    };
    route_exclusion::exclude(&mut graph, excluded, |name| in_packages(excluded, name));
    graph.id = GraphId(format!(
        "typescript:{}@{}:{}:{}:{}",
        graph.build.project,
        selected.source_revision,
        selected.target,
        selected.configuration,
        fingerprint(&graph)
    ));
    Ok(graph)
}

/// Whether the outside name `name` lies in one of `packages`: the part
/// before its `#` names the package.
fn in_packages(packages: &[String], name: &str) -> bool {
    let package = name.split('#').next().unwrap_or(name);
    packages.iter().any(|p| p == package)
}

/// The first 16 hex digits of the SHA-256 of the graph's contents. The
/// graph's maps are ordered, so its debug text is the same for the same
/// contents.
fn fingerprint(graph: &ProjectGraph) -> String {
    let digest = Sha256::digest(format!("{graph:?}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn known(modules: &BTreeMap<ModuleId, Module>, module: String) -> Result<ModuleId, AssemblyError> {
    let id = ModuleId(module);
    if modules.contains_key(&id) {
        Ok(id)
    } else {
        Err(AssemblyError::UnknownModule(id.0))
    }
}

fn span(span: records::Span) -> SourceSpan {
    SourceSpan {
        file: span.file,
        bytes: span.start..span.end,
    }
}

fn endpoint(endpoint: records::Endpoint) -> Endpoint {
    match endpoint {
        records::Endpoint::Module(id) => Endpoint::Module(ModuleId(id)),
        records::Endpoint::Item(id) => Endpoint::Item(ItemId(id)),
        records::Endpoint::Place { owner, projection } => Endpoint::Place {
            owner: ItemId(owner),
            projection,
        },
        records::Endpoint::External(name) => Endpoint::External(name),
        records::Endpoint::Unresolved(text) => Endpoint::Unresolved(text),
    }
}

fn item_kind(kind: records::ItemKind) -> ItemKind {
    match kind {
        records::ItemKind::Type => ItemKind::Type,
        records::ItemKind::Value => ItemKind::Value,
        records::ItemKind::Function => ItemKind::Function,
        records::ItemKind::Method => ItemKind::Method,
        records::ItemKind::Module => ItemKind::Module,
    }
}

fn visibility_of(visibility: records::Visibility) -> Visibility {
    match visibility {
        records::Visibility::Exported => Visibility::Exported,
        records::Visibility::Internal => Visibility::Internal,
        records::Visibility::Unknown => Visibility::Unknown,
    }
}

fn fact_kind(kind: records::FactKind) -> FactKind {
    use records::FactKind as K;
    match kind {
        K::DeclaredDependency => FactKind::DeclaredDependency,
        K::Import { local_name, usage } => FactKind::Import {
            local_name,
            usage: match usage {
                records::ImportUse::Unused => ImportUse::Unused,
                records::ImportUse::TypeOnly => ImportUse::TypeOnly,
                records::ImportUse::Value => ImportUse::Value,
            },
        },
        K::Reexport { exported_name } => FactKind::Reexport { exported_name },
        K::TypeReference => FactKind::TypeReference,
        K::ValueReference => FactKind::ValueReference,
        K::Implements => FactKind::Implements,
        K::Call { dispatch } => FactKind::Call(match dispatch {
            records::Dispatch::Direct => Dispatch::Direct,
            records::Dispatch::Contract => Dispatch::Contract,
            records::Dispatch::Indirect => Dispatch::Indirect,
            records::Dispatch::Foreign => Dispatch::Foreign,
        }),
        K::Construct => FactKind::Construct,
        K::Store => FactKind::Store,
        K::Read => FactKind::Read,
        K::Pass => FactKind::Pass,
        K::Return => FactKind::Return,
        K::Coerce => FactKind::Coerce,
        K::Capture => FactKind::Capture,
    }
}

#[cfg(test)]
mod tests;
