//! Turns the driver's records into a `ProjectGraph`. Pure: no files, no
//! processes.
//!
//! - Each workspace package is a module. Each compiled crate is a unit.
//! - A definition in a compiled crate becomes an item, or the nearest
//!   enclosing item for closures, fields and the like. A definition in a
//!   third-party crate becomes an external endpoint. So does a definition
//!   from the toolchain's own crates, such as `std`, when a fact names it;
//!   whether those facts stay is up to the grapher's exclusion list.
//! - A fact is kept when its object lies in another module or outside the
//!   project. Identical facts are kept once.
//! - Each such fact is a route of one hop: from the thing used (the fact's
//!   object) to its user (the fact's subject).
//! - Calls left at a trait get traced routes as well: see `routes`.

mod routes;

use crate::metadata::Workspace;
use project_graph::{
    Build, BuildUnit, CoverageGap, Dispatch, Endpoint, Fact, FactId, FactKind, GraphId, Hop,
    ImportUse, Item, ItemId, ItemKind, Module, ModuleId, ProjectGraph, Route, RouteEnd, RouteId,
    SourceSpan, Support, UnitId, Visibility,
};
use project_graph_rust_records as r;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct Snapshot {
    pub project: String,
    pub source_revision: String,
    pub target: String,
    pub configuration: String,
}

pub fn graph(workspace: &Workspace, crates: &[Vec<r::Record>], snapshot: Snapshot) -> ProjectGraph {
    let mut graph = ProjectGraph {
        id: GraphId(format!(
            "{}@{} {} [{}]",
            snapshot.project, snapshot.source_revision, snapshot.target, snapshot.configuration
        )),
        build: Build {
            project: snapshot.project,
            source_revision: snapshot.source_revision,
            target: snapshot.target,
            configuration: snapshot.configuration,
        },
        modules: workspace
            .members
            .iter()
            .map(|m| {
                (
                    ModuleId(m.name.clone()),
                    Module {
                        name: m.name.clone(),
                        root: m.root.clone(),
                    },
                )
            })
            .collect(),
        units: BTreeMap::new(),
        items: BTreeMap::new(),
        facts: BTreeMap::new(),
        routes: BTreeMap::new(),
        gaps: Vec::new(),
    };
    let units = Units::new(crates);
    for (unit, records) in units.iter(crates) {
        graph.units.insert(unit.id.clone(), unit.build_unit());
        for record in records {
            if let r::Record::Item(item) = record {
                graph
                    .items
                    .insert(units.item_id(&item.def), unit.item(item, &workspace.root));
            }
        }
    }
    let resolve = Resolver {
        workspace,
        units: &units,
        items: &graph.items,
        from: None,
    };
    let mut facts = BTreeSet::new();
    for (unit, records) in units.iter(crates) {
        facts.extend(declared_dependencies(workspace, unit));
        for record in records {
            match record {
                r::Record::Gap(gap) => graph.gaps.push(CoverageGap {
                    unit: Some(unit.id.clone()),
                    site: gap.span.as_ref().map(|s| site(s, &workspace.root)),
                    reason: gap.reason.clone(),
                }),
                other => facts.extend(fact(&resolve, unit, other)),
            }
        }
    }
    let traced = routes::traced(&resolve, &units, crates);
    let mut all: BTreeSet<Ordered> = facts.iter().map(|f| Ordered(f.0.clone())).collect();
    for route in &traced {
        all.insert(Ordered(route.call.clone()));
        all.extend(route.hops.iter().map(|(fact, _)| Ordered(fact.clone())));
    }
    let ids: BTreeMap<_, _> = all
        .iter()
        .enumerate()
        .map(|(index, fact)| (key(&fact.0), FactId(format!("fact:{index}"))))
        .collect();
    let id = |fact: &Fact| ids[&key(fact)].clone();
    for (index, Ordered(fact)) in facts.iter().enumerate() {
        graph.routes.insert(
            RouteId(format!("route:{index}")),
            Route {
                use_sites: vec![id(fact)],
                origin: fact.object.clone(),
                hops: vec![Hop {
                    to: fact.subject.clone(),
                    facts: vec![id(fact)],
                    support: Support::Established,
                }],
                end: RouteEnd::ReachedUse,
            },
        );
    }
    for (index, route) in traced.iter().enumerate() {
        let use_site = id(&route.call);
        graph.routes.insert(
            RouteId(format!("route:traced:{index}")),
            Route {
                use_sites: vec![use_site.clone()],
                origin: route.origin.clone(),
                hops: route
                    .hops
                    .iter()
                    .map(|(fact, possible)| Hop {
                        to: fact.object.clone(),
                        facts: vec![id(fact)],
                        support: if *possible {
                            Support::Possible { condition: None }
                        } else {
                            Support::Established
                        },
                    })
                    .collect(),
                end: match &route.stopped {
                    None => RouteEnd::ReachedUse,
                    Some(reason) => RouteEnd::Stopped {
                        at: vec![use_site],
                        reason: reason.clone(),
                    },
                },
            },
        );
    }
    graph.facts = all
        .into_iter()
        .map(|Ordered(fact)| (id(&fact), fact))
        .collect();
    graph
}

/// One compiled crate.
struct Unit {
    id: UnitId,
    record: r::UnitRecord,
}

impl Unit {
    fn module(&self) -> ModuleId {
        ModuleId(self.record.package.clone())
    }

    fn build_unit(&self) -> BuildUnit {
        let mut settings = BTreeMap::new();
        settings.insert("features".to_string(), self.record.features.join(","));
        settings.insert("target".to_string(), self.record.target_triple.clone());
        settings.insert("test".to_string(), self.record.test.to_string());
        BuildUnit {
            module: self.module(),
            target: format!("{}:{}", kind(&self.record), self.record.crate_name),
            settings,
        }
    }

    fn item(&self, item: &r::ItemRecord, root: &Path) -> Item {
        Item {
            module: self.module(),
            name: if item.def.path.is_empty() {
                item.def.crate_name.clone()
            } else {
                item.def.name.clone()
            },
            kind: match item.kind {
                r::ItemKind::Type => ItemKind::Type,
                r::ItemKind::Value => ItemKind::Value,
                r::ItemKind::Function => ItemKind::Function,
                r::ItemKind::Method => ItemKind::Method,
                r::ItemKind::Module => ItemKind::Module,
            },
            visibility: if item.exported {
                Visibility::Exported
            } else {
                Visibility::Internal
            },
            signature: item.signature.clone(),
            declaration: item.span.as_ref().map(|s| site(s, root)),
        }
    }
}

fn kind(unit: &r::UnitRecord) -> &str {
    unit.crate_types.first().map_or("lib", String::as_str)
}

/// The compiled crates, by the compiler's crate identity.
struct Units {
    by_crate: BTreeMap<String, Unit>,
}

impl Units {
    fn new(crates: &[Vec<r::Record>]) -> Self {
        let by_crate = crates
            .iter()
            .filter_map(|records| match records.first() {
                Some(r::Record::Unit(unit)) => Some((
                    unit.crate_id.clone(),
                    Unit {
                        id: UnitId(format!(
                            "{}/{}:{}{}",
                            unit.package,
                            kind(unit),
                            unit.crate_name,
                            if unit.test { "/test" } else { "" }
                        )),
                        record: unit.clone(),
                    },
                )),
                _ => None,
            })
            .collect();
        Units { by_crate }
    }

    fn iter<'a>(
        &'a self,
        crates: &'a [Vec<r::Record>],
    ) -> impl Iterator<Item = (&'a Unit, &'a [r::Record])> {
        crates.iter().filter_map(|records| match records.first() {
            Some(r::Record::Unit(unit)) => Some((&self.by_crate[&unit.crate_id], &records[1..])),
            _ => None,
        })
    }

    fn item_id(&self, def: &r::Def) -> ItemId {
        let unit = self
            .by_crate
            .get(&def.crate_id)
            .map_or(def.crate_name.as_str(), |u| u.id.0.as_str());
        ItemId(format!("{unit}{}", def.path))
    }
}

#[derive(Clone, Copy)]
struct Resolver<'a> {
    workspace: &'a Workspace,
    units: &'a Units,
    items: &'a BTreeMap<ItemId, Item>,
    /// The package whose source is being resolved, which decides how an
    /// outside definition is named.
    from: Option<&'a str>,
}

impl<'a> Resolver<'a> {
    /// This resolver, for definitions named in `unit`'s source.
    fn at(self, unit: &'a Unit) -> Self {
        Resolver {
            from: Some(&unit.record.package),
            ..self
        }
    }

    /// Like `endpoint`, but a definition from the toolchain's own crates,
    /// such as `std`, is an outside endpoint named from its crate, as
    /// `core::fmt::Display`. Facts name the toolchain this way, so a
    /// grapher's exclusion list decides whether they stay; traced routes do
    /// not follow values through the toolchain's traits.
    fn fact_endpoint(&self, def: &r::Def) -> Option<(Endpoint, Option<ModuleId>)> {
        self.endpoint(def).or_else(|| {
            Some((
                Endpoint::External(external_name(None, &def.crate_name, def)),
                None,
            ))
        })
    }

    /// The endpoint naming `def`, and the module it belongs to when it is in
    /// the project. `None` for definitions from the toolchain's crates.
    fn endpoint(&self, def: &r::Def) -> Option<(Endpoint, Option<ModuleId>)> {
        if let Some(unit) = self.units.by_crate.get(&def.crate_id) {
            let mut path = def.path.as_str();
            loop {
                let id = self.units.item_id(&r::Def {
                    path: path.to_string(),
                    ..def.clone()
                });
                if self.items.contains_key(&id) {
                    return Some((Endpoint::Item(id), Some(unit.module())));
                }
                match path.rfind("::") {
                    Some(end) => path = &path[..end],
                    None => return Some((Endpoint::Module(unit.module()), Some(unit.module()))),
                }
            }
        }
        let package = self.workspace.third_party.get(&def.crate_name)?;
        let through = self
            .from
            .and_then(|member| self.workspace.reached_through(member, package));
        Some((
            Endpoint::External(external_name(through.as_deref(), package, def)),
            None,
        ))
    }
}

/// A third-party definition's name. It starts with the dependency the
/// project declares, `through`, when that is known, then the package that
/// defines it: `serde::de::Deserialize` for a direct dependency,
/// `serde::serde_core::de::Deserialize` for one reached through `serde`, or
/// `thiserror::<&T as thiserror::AsDisplay>::as_display` for an
/// implementation's method.
fn external_name(through: Option<&str>, package: &str, def: &r::Def) -> String {
    let in_crate = def
        .name
        .strip_prefix(&def.crate_name)
        .and_then(|rest| rest.strip_prefix("::"));
    let defined = match in_crate {
        Some(rest) => format!("{package}::{rest}"),
        None => format!("{package}::{}", def.name),
    };
    match through {
        Some(direct) if direct != package => format!("{direct}::{defined}"),
        _ => defined,
    }
}

/// Facts ordered for stable identifiers.
#[derive(PartialEq, Eq)]
struct Ordered(Fact);

impl Ord for Ordered {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        key(&self.0).cmp(&key(&other.0))
    }
}

impl PartialOrd for Ordered {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn key(fact: &Fact) -> (String, Option<(String, usize, usize)>, String) {
    (
        fact.unit.0.clone(),
        fact.site
            .as_ref()
            .map(|s| (s.file.clone(), s.bytes.start, s.bytes.end)),
        format!("{:?} {:?} {:?}", fact.kind, fact.subject, fact.object),
    )
}

fn declared_dependencies(workspace: &Workspace, unit: &Unit) -> Vec<Ordered> {
    let Some(member) = workspace
        .members
        .iter()
        .find(|m| m.name == unit.record.package)
    else {
        return Vec::new();
    };
    unit.record
        .externs
        .iter()
        .filter_map(|name| member.dependencies.get(name))
        // A package can depend on itself, for example to test with features.
        .filter(|dep| dep.package != unit.record.package)
        .map(|dep| {
            Ordered(Fact {
                unit: unit.id.clone(),
                subject: Endpoint::Module(unit.module()),
                kind: FactKind::DeclaredDependency,
                object: if dep.member {
                    Endpoint::Module(ModuleId(dep.package.clone()))
                } else {
                    Endpoint::External(dep.package.clone())
                },
                site: None,
            })
        })
        .collect()
}

fn fact(resolve: &Resolver, unit: &Unit, record: &r::Record) -> Option<Ordered> {
    let resolve = resolve.at(unit);
    let root = &resolve.workspace.root;
    let (subject, kind, object, span) = match record {
        r::Record::Use(u) => (
            resolve.fact_endpoint(&u.module)?,
            if u.public {
                FactKind::Reexport {
                    exported_name: u.local_name.clone(),
                }
            } else {
                FactKind::Import {
                    local_name: u.local_name.clone(),
                    usage: ImportUse::Undetermined,
                }
            },
            resolve.fact_endpoint(&u.target)?,
            &u.span,
        ),
        r::Record::Reference(reference) => (
            resolve.fact_endpoint(&reference.owner)?,
            match reference.kind {
                r::ReferenceKind::Type => FactKind::TypeReference,
                r::ReferenceKind::Value => FactKind::ValueReference,
            },
            resolve.fact_endpoint(&reference.target)?,
            &reference.span,
        ),
        r::Record::Impl(implementation) => (
            match &implementation.self_type {
                Some(def) => resolve.fact_endpoint(def)?,
                None => (Endpoint::Module(unit.module()), Some(unit.module())),
            },
            FactKind::Implements,
            resolve.fact_endpoint(&implementation.contract)?,
            &implementation.span,
        ),
        r::Record::Call(call) => (
            resolve.fact_endpoint(&call.owner)?,
            FactKind::Call(match call.dispatch {
                r::Dispatch::Direct => Dispatch::Direct,
                r::Dispatch::SelectedImplementation => Dispatch::SelectedImplementation,
                r::Dispatch::Contract => Dispatch::Contract,
                r::Dispatch::Indirect => Dispatch::Indirect,
                r::Dispatch::Foreign => Dispatch::Foreign,
            }),
            match &call.callee {
                Some(callee) => resolve.fact_endpoint(callee)?,
                None => (Endpoint::Unresolved("function pointer".into()), None),
            },
            &call.span,
        ),
        r::Record::Unit(_)
        | r::Record::Item(_)
        | r::Record::Gap(_)
        | r::Record::Flow(_)
        | r::Record::Receiver(_)
        | r::Record::ClosureBinding(_)
        | r::Record::ClosureCall(_) => return None,
    };
    let ((subject, subject_module), (object, object_module)) = (subject, object);
    // Uses inside one module are that module's own business.
    if object_module.is_some() && object_module == subject_module {
        return None;
    }
    Some(Ordered(Fact {
        unit: unit.id.clone(),
        subject,
        kind,
        object,
        site: Some(site(span, root)),
    }))
}

/// A span with its file relative to the workspace root.
fn site(span: &r::Span, root: &Path) -> SourceSpan {
    let file = Path::new(&span.file);
    SourceSpan {
        file: file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned(),
        bytes: span.lo..span.hi,
    }
}

#[cfg(test)]
mod tests;
