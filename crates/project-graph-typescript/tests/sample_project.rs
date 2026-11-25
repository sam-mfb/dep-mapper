//! Builds the graph for `sample/` with the real collector and checks its
//! modules, items, facts and routes. Needs Node and the collector's
//! dependencies (`npm ci` in `collector/`).
//!
//! The sample: `backup-api` declares the `Backups` interface. `backup-os`
//! implements it as `OsBackups`. `installer` holds a `Backups` and calls
//! `write` on it. `app` builds an `Installer` with either `OsBackups` or its
//! own `MemoryBackups`, which implements `Backups` without saying so, and
//! also calls `OsBackups.prune` itself. `clock` is a third-party package.

use project_graph::*;
use project_graph_typescript::{
    AS_WRITTEN, GitRevision, NodeCollector, RevisionError, SourceRevision, TraceLimits,
    TypeScriptGrapher,
};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct FixedRevision;

impl SourceRevision for FixedRevision {
    fn revision(&self, _root: &Path) -> Result<String, RevisionError> {
        Ok("fixture".into())
    }
}

fn sample_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("sample")
}

fn selection(configuration: &str) -> BuildSelection {
    BuildSelection {
        root: sample_root(),
        target: "tsconfig.json".into(),
        configuration: configuration.into(),
    }
}

fn grapher() -> TypeScriptGrapher {
    TypeScriptGrapher::new(
        Box::new(NodeCollector::bundled(PathBuf::from("node"))),
        Box::new(FixedRevision),
        TraceLimits::STANDARD,
    )
}

fn graph() -> &'static ProjectGraph {
    static GRAPH: OnceLock<ProjectGraph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        grapher()
            .build_graph(&selection(AS_WRITTEN))
            .expect("the sample builds")
    })
}

const API: &str = "@sample/backup-api/src/index.ts";
const OS: &str = "@sample/backup-os/src/os.ts";
const INSTALLER: &str = "@sample/installer/src/index.ts";
const MAIN: &str = "@sample/app/src/main.ts";

fn id(file: &str, name: &str) -> ItemId {
    ItemId(format!("{file}#{name}"))
}

fn item(file: &str, name: &str) -> Endpoint {
    Endpoint::Item(id(file, name))
}

fn place(file: &str, owner: &str, projection: &str) -> Endpoint {
    Endpoint::Place {
        owner: id(file, owner),
        projection: projection.into(),
    }
}

/// The source text a span covers.
fn text(site: &SourceSpan) -> String {
    let source = std::fs::read(sample_root().join(&site.file)).unwrap();
    String::from_utf8(source[site.bytes.clone()].to_vec()).unwrap()
}

fn facts_where(test: impl Fn(&Fact) -> bool) -> Vec<(&'static FactId, &'static Fact)> {
    graph().facts.iter().filter(|(_, f)| test(f)).collect()
}

fn only_fact(test: impl Fn(&Fact) -> bool) -> (&'static FactId, &'static Fact) {
    let found = facts_where(test);
    assert_eq!(found.len(), 1, "expected one fact, found {found:#?}");
    found[0]
}

fn hop_kinds(route: &Route) -> Vec<&'static FactKind> {
    route
        .hops
        .iter()
        .map(|hop| &graph().facts[&hop.fact].kind)
        .collect()
}

#[test]
fn the_graph_names_the_selected_build() {
    let graph = graph();
    assert_eq!(graph.build.project, "backup-sample");
    assert_eq!(graph.build.source_revision, "fixture");
    assert!(selection(AS_WRITTEN).describes(&graph.build));
    assert!(graph.gaps.is_empty(), "{:#?}", graph.gaps);
}

#[test]
fn another_configuration_is_refused() {
    let error = grapher().build_graph(&selection("debug")).unwrap_err();
    assert!(error.message.contains("configuration must be \"tsconfig\""));
}

#[test]
fn each_workspace_package_is_a_module_with_one_unit() {
    let graph = graph();
    let modules: Vec<_> = graph
        .modules
        .iter()
        .map(|(id, m)| (id.0.as_str(), m.root.as_str()))
        .collect();
    assert_eq!(
        modules,
        [
            ("@sample/app", "packages/app"),
            ("@sample/backup-api", "packages/backup-api"),
            ("@sample/backup-os", "packages/backup-os"),
            ("@sample/installer", "packages/installer"),
        ]
    );
    let unit = &graph.units[&UnitId("@sample/installer:tsconfig.json".into())];
    assert_eq!(unit.settings["typescript"], "6.0.3");
    assert_eq!(unit.settings["strict"], "true");
}

#[test]
fn items_carry_kind_visibility_signature_and_declaration() {
    let items = &graph().items;
    let check = |item: &ItemId, kind: ItemKind, visibility: Visibility| {
        let found = &items[item];
        assert_eq!(
            (&found.kind, &found.visibility),
            (&kind, &visibility),
            "{item:?}"
        );
    };
    check(&id(API, "Backups"), ItemKind::Type, Visibility::Exported);
    check(
        &id(API, "Backups.write"),
        ItemKind::Method,
        Visibility::Exported,
    );
    check(&id(API, "Label"), ItemKind::Type, Visibility::Exported);
    // Exported from the package through index.ts's re-export.
    check(&id(OS, "OsBackups"), ItemKind::Type, Visibility::Exported);
    check(&id(OS, "stamp"), ItemKind::Function, Visibility::Internal);
    check(
        &id(INSTALLER, "Installer.constructor"),
        ItemKind::Method,
        Visibility::Exported,
    );
    check(
        &id(INSTALLER, "installWith"),
        ItemKind::Function,
        Visibility::Exported,
    );
    check(
        &id(MAIN, "MemoryBackups"),
        ItemKind::Type,
        Visibility::Internal,
    );
    check(&ItemId(OS.into()), ItemKind::Module, Visibility::Internal);
    check(
        &ItemId("@sample/backup-os/src/index.ts".into()),
        ItemKind::Module,
        Visibility::Exported,
    );

    let write = &items[&id(OS, "OsBackups.write")];
    assert_eq!(write.module, ModuleId("@sample/backup-os".into()));
    assert_eq!(
        write.signature.as_deref(),
        Some("write(name: string): string")
    );
    let declaration = write.declaration.as_ref().unwrap();
    assert!(text(declaration).starts_with("write(name: string): string {"));
}

#[test]
fn imports_record_how_the_name_is_used() {
    let usage = |file: &str, name: &str| {
        let (_, fact) = only_fact(|f| {
            f.subject == Endpoint::Item(ItemId(file.into()))
                && matches!(&f.kind, FactKind::Import { local_name, .. } if local_name == name)
        });
        match &fact.kind {
            FactKind::Import { usage, .. } => usage.clone(),
            _ => unreachable!(),
        }
    };
    assert_eq!(usage(OS, "Label"), ImportUse::Unused);
    assert_eq!(usage(OS, "Backups"), ImportUse::TypeOnly);
    assert_eq!(usage(OS, "now"), ImportUse::Value);
    assert_eq!(usage(MAIN, "OsBackups"), ImportUse::Value);
    assert_eq!(usage(MAIN, "Backups"), ImportUse::TypeOnly);

    let (_, now) = only_fact(
        |f| matches!(&f.kind, FactKind::Import { local_name, .. } if local_name == "now"),
    );
    assert_eq!(now.object, Endpoint::External("clock#now".into()));
}

#[test]
fn imports_inside_one_package_are_not_facts() {
    assert!(
        facts_where(
            |f| matches!(&f.kind, FactKind::Import { .. } | FactKind::Reexport { .. })
                && f.object == item(OS, "OsBackups")
                && f.subject == Endpoint::Item(ItemId("@sample/backup-os/src/index.ts".into()))
        )
        .is_empty()
    );
}

#[test]
fn a_reexport_names_what_it_exposes() {
    let (_, fact) = only_fact(|f| matches!(f.kind, FactKind::Reexport { .. }));
    assert_eq!(fact.subject, Endpoint::Item(ItemId(INSTALLER.into())));
    assert_eq!(fact.object, item(API, "Backups"));
    assert_eq!(
        fact.kind,
        FactKind::Reexport {
            exported_name: "Backups".into()
        }
    );
}

#[test]
fn declared_and_structural_implementations_are_both_facts() {
    let implements = |class: Endpoint| {
        only_fact(|f| {
            f.kind == FactKind::Implements && f.subject == class && f.object == item(API, "Backups")
        })
        .1
    };
    let declared = implements(item(OS, "OsBackups"));
    assert_eq!(text(declared.site.as_ref().unwrap()), "Backups");
    let witnessed = implements(item(MAIN, "MemoryBackups"));
    assert_eq!(
        text(witnessed.site.as_ref().unwrap()),
        "new MemoryBackups()"
    );
}

#[test]
fn calls_record_their_dispatch() {
    let call = |caller: Endpoint, callee: Endpoint| {
        let (_, fact) = only_fact(|f| {
            matches!(f.kind, FactKind::Call(_)) && f.subject == caller && f.object == callee
        });
        match &fact.kind {
            FactKind::Call(dispatch) => dispatch.clone(),
            _ => unreachable!(),
        }
    };
    assert_eq!(
        call(
            item(INSTALLER, "Installer.install"),
            item(API, "Backups.write")
        ),
        Dispatch::Contract
    );
    assert_eq!(
        call(item(MAIN, "main"), item(OS, "OsBackups.prune")),
        Dispatch::Direct
    );
    assert_eq!(
        call(item(OS, "stamp"), Endpoint::External("clock#now".into())),
        Dispatch::Foreign
    );
    // No constructor is written, so the call names the class.
    assert_eq!(
        call(item(MAIN, "main"), item(OS, "OsBackups")),
        Dispatch::Direct
    );
    assert_eq!(
        call(item(MAIN, "main"), item(INSTALLER, "Installer.constructor")),
        Dispatch::Direct
    );
}

#[test]
fn a_package_dependency_outside_the_workspace_is_external() {
    let (_, fact) = only_fact(|f| {
        f.kind == FactKind::DeclaredDependency
            && f.subject == Endpoint::Module(ModuleId("@sample/app".into()))
            && f.object == Endpoint::External("clock".into())
    });
    let site = fact.site.as_ref().unwrap();
    assert_eq!(site.file, "packages/app/package.json");
    assert_eq!(text(site), "\"clock\"");
}

#[test]
fn an_implementation_entering_an_interface_slot_is_a_conditional_coercion() {
    let (id, fact) =
        only_fact(|f| f.kind == FactKind::Coerce && f.object == place(MAIN, "main", "backups"));
    assert_eq!(fact.subject, place(MAIN, "main", "os"));
    assert_eq!(fact.object, place(MAIN, "main", "backups"));
    assert_eq!(text(fact.site.as_ref().unwrap()), "os");
    let hop = graph()
        .routes
        .values()
        .flat_map(|r| &r.hops)
        .find(|h| &h.fact == id)
        .unwrap();
    assert_eq!(
        hop.support,
        Support::Possible {
            condition: Some("!dryRun".into())
        }
    );
}

/// The call `this.backups.write(name)` inside `Installer.install`.
fn install_call() -> &'static FactId {
    only_fact(|f| {
        f.subject == item(INSTALLER, "Installer.install") && f.object == item(API, "Backups.write")
    })
    .0
}

fn routes_to(call: &FactId) -> Vec<&'static Route> {
    graph()
        .routes
        .iter()
        .filter(|(id, r)| id.0.starts_with("value:") && r.use_site.as_ref() == Some(call))
        .map(|(_, r)| r)
        .collect()
}

#[test]
fn both_providers_reach_the_installers_contract_call() {
    let routes = routes_to(install_call());
    assert_eq!(routes.len(), 2);
    let from = |origin: Endpoint| routes.iter().find(|r| r.origin == origin).unwrap();

    let memory = from(item(MAIN, "MemoryBackups"));
    assert_eq!(memory.end, RouteEnd::ReachedUse);
    assert_eq!(
        hop_kinds(memory),
        [
            &FactKind::Construct,
            &FactKind::Pass,
            &FactKind::Store,
            &FactKind::Pass
        ]
    );
    let stops: Vec<_> = memory.hops.iter().map(|h| h.to.clone()).collect();
    assert_eq!(
        stops,
        [
            place(MAIN, "main", "backups"),
            place(INSTALLER, "Installer.constructor", "backups"),
            place(INSTALLER, "Installer", "backups"),
            place(API, "Backups.write", "self"),
        ]
    );
    assert_eq!(
        memory.hops[0].support,
        Support::Possible {
            condition: Some("dryRun".into())
        }
    );

    let os = from(item(OS, "OsBackups"));
    assert_eq!(
        hop_kinds(os),
        [
            &FactKind::Construct,
            &FactKind::Coerce,
            &FactKind::Pass,
            &FactKind::Store,
            &FactKind::Pass
        ]
    );
}

#[test]
fn the_program_also_uses_its_provider_directly() {
    let (prune, _) = only_fact(|f| {
        f.object == item(OS, "OsBackups.prune") && matches!(f.kind, FactKind::Call(_))
    });
    let routes = routes_to(prune);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].origin, item(OS, "OsBackups"));
    assert_eq!(
        hop_kinds(routes[0]),
        [&FactKind::Construct, &FactKind::Pass]
    );
    let direct = &graph().routes[&RouteId(format!("use:{}", prune.0))];
    assert_eq!(direct.hops[0].to, item(MAIN, "main"));
}

#[test]
fn a_receiver_nobody_supplies_stops_the_trace() {
    let (call, _) = only_fact(|f| {
        f.subject == item(INSTALLER, "installWith") && matches!(f.kind, FactKind::Call(_))
    });
    let routes = routes_to(call);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].origin, place(INSTALLER, "installWith", "backups"));
    let RouteEnd::Stopped { reason, .. } = &routes[0].end else {
        panic!("expected a stopped route")
    };
    assert_eq!(reason, "no flow reaches the receiver from a known origin");
}

#[test]
fn each_implementer_is_a_receiver_candidate_for_the_contract_call() {
    let call = install_call();
    let bounds: Vec<_> =
        facts_where(|f| matches!(&f.kind, FactKind::ReceiverBound { call: c, .. } if c == call))
            .into_iter()
            .map(|(_, f)| (f.object.clone(), f.kind.clone()))
            .collect();
    let implementers = [
        item(OS, "OsBackups"),
        item(MAIN, "MemoryBackups"),
        item(ECHO, "SpareBackups"),
        item(REGISTRY, "quietBackups"),
        item(REGISTRY, "FallbackBackups"),
    ];
    assert_eq!(bounds.len(), implementers.len());
    for (object, kind) in bounds {
        assert!(implementers.contains(&object), "{object:?}");
        assert!(matches!(
            kind,
            FactKind::ReceiverBound {
                basis: ReceiverBasis::BuildCandidate,
                ..
            }
        ));
    }
}

#[test]
fn direct_uses_between_packages_are_one_hop_routes() {
    let graph = graph();
    let unused = only_fact(
        |f| matches!(&f.kind, FactKind::Import { local_name, .. } if local_name == "Label"),
    )
    .0;
    let route = &graph.routes[&RouteId(format!("use:{}", unused.0))];
    assert_eq!(route.origin, item(API, "Label"));
    assert_eq!(route.hops[0].to, Endpoint::Item(ItemId(OS.into())));
    // A call inside one package is not a direct-use route.
    let stamp = only_fact(|f| f.object == item(OS, "stamp")).0;
    assert!(
        !graph
            .routes
            .contains_key(&RouteId(format!("use:{}", stamp.0)))
    );
}

#[test]
fn every_route_cites_facts_in_the_graph() {
    let graph = graph();
    for (id, route) in &graph.routes {
        for hop in &route.hops {
            assert!(
                graph.facts.contains_key(&hop.fact),
                "{id:?} cites {:?}",
                hop.fact
            );
        }
        if let Some(call) = &route.use_site {
            assert!(graph.facts.contains_key(call), "{id:?} uses {call:?}");
        }
    }
}

#[test]
fn a_missing_tsconfig_is_a_graphing_error() {
    let mut selection = selection(AS_WRITTEN);
    selection.target = "missing.json".into();
    let error = grapher().build_graph(&selection).unwrap_err();
    assert!(
        error.message.contains("the TypeScript collector failed"),
        "{}",
        error.message
    );
    assert!(error.message.contains("missing.json"), "{}", error.message);
}

#[test]
fn a_collector_without_its_dependencies_says_how_to_install_them() {
    let collector = NodeCollector {
        node: PathBuf::from("node"),
        collector: std::env::temp_dir().join("srs-no-such-collector"),
    };
    let grapher = TypeScriptGrapher::new(
        Box::new(collector),
        Box::new(FixedRevision),
        TraceLimits::STANDARD,
    );
    let error = grapher.build_graph(&selection(AS_WRITTEN)).unwrap_err();
    assert!(error.message.contains("run `npm ci`"), "{}", error.message);
}

#[test]
fn workspace_files_reached_by_imports_are_analyzed() {
    let mut app_only = selection(AS_WRITTEN);
    app_only.target = "tsconfig.app.json".into();
    let narrow = grapher()
        .build_graph(&app_only)
        .expect("the app-only build");
    let full = graph();
    assert!(narrow.gaps.is_empty(), "{:#?}", narrow.gaps);
    assert_eq!(
        narrow.items.keys().collect::<Vec<_>>(),
        full.items.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        narrow.facts.keys().collect::<Vec<_>>(),
        full.facts.keys().collect::<Vec<_>>()
    );
    assert!(narrow.items.contains_key(&id(OS, "OsBackups")));
}

/// Copies the sample into a fresh temporary directory.
fn sample_copy(name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("srs-sample-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&sample_root(), &dir);
    dir
}

#[test]
fn builds_of_different_source_under_one_revision_get_different_ids() {
    let root = sample_copy("fingerprint");
    let probe = root.join("packages/backup-api/src/probe.ts");
    let build = || {
        let mut selection = selection(AS_WRITTEN);
        selection.root = root.clone();
        grapher().build_graph(&selection).unwrap().id
    };
    std::fs::write(&probe, "export const probeOne = 1;\n").unwrap();
    let one = build();
    assert_eq!(build(), one);
    std::fs::write(&probe, "export const probeTwo = 2;\n").unwrap();
    let two = build();
    std::fs::remove_dir_all(&root).unwrap();
    assert_ne!(one, two);
}

/// The call `backups.write(name)` inside the closure `installLater` returns.
fn later_call() -> &'static FactId {
    only_fact(|f| {
        f.subject == item(INSTALLER, "installLater") && matches!(f.kind, FactKind::Call(_))
    })
    .0
}

#[test]
fn a_destructured_parameter_reads_the_field_it_names() {
    let (_, read) = only_fact(|f| {
        f.kind == FactKind::Read && f.object == place(INSTALLER, "installLater", "backups")
    });
    assert_eq!(read.subject, place(INSTALLER, "InstallParts", "backups"));
    assert_eq!(text(read.site.as_ref().unwrap()), "backups");
}

#[test]
fn a_closure_captures_the_variables_it_uses() {
    let captures = facts_where(|f| f.kind == FactKind::Capture);
    assert_eq!(captures.len(), 2, "{captures:#?}");
    let (_, backups) = only_fact(|f| {
        f.kind == FactKind::Capture && f.subject == place(INSTALLER, "installLater", "backups")
    });
    assert_eq!(
        text(backups.site.as_ref().unwrap()),
        "() => backups.write(name)"
    );
    let Endpoint::Place { projection, .. } = &backups.object else {
        panic!("a capture moves into a place")
    };
    let offset = backups.site.as_ref().unwrap().bytes.start;
    assert_eq!(projection, &format!("backups@closure:{offset}"));
}

#[test]
fn a_value_route_runs_through_an_object_literal_a_pattern_and_a_capture() {
    let routes = routes_to(later_call());
    assert_eq!(routes.len(), 1, "{routes:#?}");
    let route = routes[0];
    assert_eq!(route.origin, item(OS, "OsBackups"));
    assert_eq!(route.end, RouteEnd::ReachedUse);
    assert_eq!(
        hop_kinds(route),
        [
            &FactKind::Construct,
            &FactKind::Coerce,
            &FactKind::Read,
            &FactKind::Capture,
            &FactKind::Pass
        ]
    );
    assert_eq!(
        route.hops[1].to,
        place(INSTALLER, "InstallParts", "backups")
    );
    assert_eq!(
        route.hops[2].to,
        place(INSTALLER, "installLater", "backups")
    );
}

#[test]
fn an_array_pattern_takes_each_element_of_its_literal_and_skips_an_impossible_default() {
    let (_, store) = only_fact(|f| f.object == place(MAIN, "main", "spare"));
    assert_eq!(store.kind, FactKind::Store);
    assert_eq!(store.subject, place(MAIN, "main", "os"));

    let (call, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_))
            && f.site
                .as_ref()
                .is_some_and(|site| text(site) == "spare.write(\"spare\")")
    });
    let routes = routes_to(call);
    assert_eq!(routes.len(), 1, "{routes:#?}");
    assert_eq!(routes[0].origin, item(OS, "OsBackups"));
    assert!(
        routes[0]
            .hops
            .iter()
            .all(|h| h.support == Support::Established)
    );
}

#[test]
fn an_object_pattern_default_and_the_field_read_are_each_conditional() {
    let (call, _) = only_fact(|f| {
        f.subject == item(MAIN, "main")
            && f.object == item(API, "Backups.prune")
            && matches!(f.kind, FactKind::Call(_))
    });
    let routes = routes_to(call);
    assert_eq!(routes.len(), 2, "{routes:#?}");
    let possible = |condition: &str| Support::Possible {
        condition: Some(condition.into()),
    };

    let default = routes
        .iter()
        .find(|r| r.origin == item(MAIN, "MemoryBackups"))
        .expect("a route from the default");
    assert_eq!(default.end, RouteEnd::ReachedUse);
    assert_eq!(default.hops[0].to, place(MAIN, "pick", "backups"));
    assert_eq!(default.hops[0].support, possible("backups === undefined"));

    let supplied = routes
        .iter()
        .find(|r| r.origin == item(OS, "OsBackups"))
        .expect("a route through the field");
    assert_eq!(supplied.end, RouteEnd::ReachedUse);
    let read = supplied
        .hops
        .iter()
        .find(|h| graph().facts[&h.fact].kind == FactKind::Read)
        .expect("the pattern reads the field");
    assert_eq!(read.to, place(MAIN, "pick", "backups"));
    assert_eq!(read.support, possible("backups !== undefined"));
}

#[test]
fn a_default_after_a_spread_is_only_possible() {
    let into_spread = facts_where(|f| f.object == place(MAIN, "main", "spread"));
    assert_eq!(into_spread.len(), 1, "{into_spread:#?}");
    let (construct, fact) = into_spread[0];
    assert_eq!(fact.kind, FactKind::Construct);
    assert_eq!(fact.subject, item(MAIN, "MemoryBackups"));
    let hop = graph()
        .routes
        .values()
        .flat_map(|r| &r.hops)
        .find(|h| &h.fact == construct)
        .expect("the default's move is on a route");
    assert_eq!(
        hop.support,
        Support::Possible {
            condition: Some("[0] === undefined".into())
        }
    );
}

const ECHO: &str = "@sample/app/src/echo.ts";

/// The origins of the value routes for the call whose source text is `call`.
fn origins_for(call: &str) -> Vec<Endpoint> {
    let (id, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
    });
    routes_to(id).iter().map(|r| r.origin.clone()).collect()
}

#[test]
fn a_result_passed_straight_into_another_call_keeps_its_own_origin() {
    assert_eq!(
        origins_for("first.write(\"first\")"),
        [item(OS, "OsBackups")]
    );
    assert_eq!(
        origins_for("second.write(\"second\")"),
        [item(ECHO, "SpareBackups")]
    );
}

#[test]
fn a_call_result_used_as_a_receiver_keeps_its_own_origin() {
    assert_eq!(
        origins_for("echo(new OsBackups()).write(\"first\")"),
        [item(OS, "OsBackups")]
    );
    assert_eq!(
        origins_for("echo(new SpareBackups()).write(\"second\")"),
        [item(ECHO, "SpareBackups")]
    );
}

#[test]
fn source_outside_git_is_named_by_its_contents() {
    let root = sample_copy("unversioned");
    let grapher = TypeScriptGrapher::new(
        Box::new(NodeCollector::bundled(PathBuf::from("node"))),
        Box::new(GitRevision { git: "git".into() }),
        TraceLimits::STANDARD,
    );
    let build = || {
        let mut selection = selection(AS_WRITTEN);
        selection.root = root.clone();
        grapher
            .build_graph(&selection)
            .map(|g| g.build.source_revision)
    };
    let probe = root.join("packages/backup-api/src/probe.ts");
    std::fs::write(&probe, "export const probeOne = 1;\n").unwrap();
    let one = build();
    let again = build();
    std::fs::write(&probe, "export const probeTwo = 2;\n").unwrap();
    let two = build();
    std::fs::remove_dir_all(&root).unwrap();

    let one = one.expect("a project outside Git still maps");
    assert!(one.starts_with("unversioned-"), "{one}");
    assert_eq!(one.len(), "unversioned-".len() + 16);
    assert_eq!(again.unwrap(), one);
    assert_ne!(two.unwrap(), one);
}

#[test]
fn an_inherited_tsconfig_is_part_of_the_content_revision() {
    let root = sample_copy("extends");
    let grapher = TypeScriptGrapher::new(
        Box::new(NodeCollector::bundled(PathBuf::from("node"))),
        Box::new(GitRevision { git: "git".into() }),
        TraceLimits::STANDARD,
    );
    let build = || {
        let mut selection = selection(AS_WRITTEN);
        selection.root = root.clone();
        selection.target = "tsconfig.app.json".into();
        let graph = grapher.build_graph(&selection).unwrap();
        let unit = graph.units.values().next().unwrap();
        (
            graph.build.source_revision.clone(),
            unit.settings["strict"].clone(),
        )
    };
    let (strict_revision, strict) = build();
    // Only the parent config changes; tsconfig.app.json extends it.
    let parent = root.join("tsconfig.json");
    let text = std::fs::read_to_string(&parent).unwrap();
    std::fs::write(
        &parent,
        text.replace("\"strict\": true", "\"strict\": false"),
    )
    .unwrap();
    let (loose_revision, loose) = build();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!((strict.as_str(), loose.as_str()), ("true", "false"));
    assert!(
        strict_revision.starts_with("unversioned-"),
        "{strict_revision}"
    );
    assert_ne!(strict_revision, loose_revision);
}

const REGISTRY: &str = "@sample/app/src/registry.ts";

#[test]
fn a_typed_object_literal_implements_its_interface() {
    let (_, fact) = only_fact(|f| {
        f.kind == FactKind::Implements && f.subject == item(REGISTRY, "quietBackups")
    });
    assert_eq!(fact.object, item(API, "Backups"));
    assert_eq!(text(fact.site.as_ref().unwrap()), "Backups");
}

/// Each call in the registry reaches both entries of its array, through the
/// place shared by arrays of `Backups`, so each route is possible.
#[test]
fn array_elements_reach_callbacks_loops_and_find() {
    let both = [item(REGISTRY, "quietBackups"), item(OS, "OsBackups")];
    for call in [
        "each.write(\"all\")",
        "item.prune(1)",
        "candidate.write(\"probe\")",
        "first?.write(\"first\")",
    ] {
        let (id, _) = only_fact(|f| {
            matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
        });
        let routes = routes_to(id);
        let mut origins: Vec<_> = routes.iter().map(|r| format!("{:?}", r.origin)).collect();
        origins.sort();
        let mut expected: Vec<_> = both.iter().map(|o| format!("{o:?}")).collect();
        expected.sort();
        assert_eq!(origins, expected, "{call}");
        let elements = place(API, "Backups", "[element]");
        for route in routes {
            let hop = route
                .hops
                .iter()
                .find(|h| h.to == elements)
                .unwrap_or_else(|| panic!("{call}: a hop into the element place"));
            assert_eq!(hop.support, Support::Possible { condition: None }, "{call}");
        }
    }
}

#[test]
fn each_side_of_or_is_a_conditional_move() {
    let routes = routes_to(
        only_fact(|f| {
            matches!(f.kind, FactKind::Call(_))
                && f.site
                    .as_ref()
                    .is_some_and(|s| text(s) == "chosen.write(\"one\")")
        })
        .0,
    );
    let fallback = routes
        .iter()
        .find(|r| r.origin == item(REGISTRY, "quietBackups"))
        .expect("a route from the fallback");
    assert_eq!(fallback.hops[0].to, place(REGISTRY, "writeOne", "chosen"));
    assert_eq!(
        fallback.hops[0].support,
        Support::Possible {
            condition: Some("!(given)".into())
        }
    );
    let given = facts_where(|f| {
        f.subject == place(REGISTRY, "writeOne", "given")
            && f.object == place(REGISTRY, "writeOne", "chosen")
    });
    assert_eq!(given.len(), 1, "the given value is the other side");
}

#[test]
fn an_unbuilt_workspace_package_resolves_to_its_source_entry() {
    let root = sample_copy("unbuilt");
    // Without the `paths` mappings, `@sample/...` imports have nothing to
    // resolve to but the packages' own manifests, as in a fresh checkout.
    let config = root.join("tsconfig.json");
    let text = std::fs::read_to_string(&config).unwrap();
    let start = text.find("\"@sample/backup-api\"").unwrap();
    let end = text.find("\"clock\"").unwrap();
    std::fs::write(&config, format!("{}{}", &text[..start], &text[end..])).unwrap();
    let mut selection = selection(AS_WRITTEN);
    selection.root = root.clone();
    let unbuilt = grapher().build_graph(&selection);
    std::fs::remove_dir_all(&root).unwrap();

    let unbuilt = unbuilt.expect("the unbuilt copy builds");
    assert!(unbuilt.gaps.is_empty(), "{:#?}", unbuilt.gaps);
    assert_eq!(
        unbuilt.facts.keys().collect::<Vec<_>>(),
        graph().facts.keys().collect::<Vec<_>>()
    );
}

#[test]
fn reduce_gives_elements_to_its_second_parameter_not_its_accumulator() {
    let call_routes = |call: &str| {
        let (id, _) = only_fact(|f| {
            matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
        });
        routes_to(id)
    };
    let mut element_origins: Vec<_> = call_routes("next.prune(0)")
        .iter()
        .map(|r| format!("{:?}", r.origin))
        .collect();
    element_origins.sort();
    let mut expected = vec![
        format!("{:?}", item(OS, "OsBackups")),
        format!("{:?}", item(REGISTRY, "quietBackups")),
    ];
    expected.sort();
    assert_eq!(element_origins, expected);

    let accumulator = call_routes("total.count()");
    assert!(
        accumulator
            .iter()
            .all(|r| matches!(r.end, RouteEnd::Stopped { .. })),
        "no array element reaches the accumulator: {accumulator:#?}"
    );
}

#[test]
fn an_array_pattern_over_a_variable_reads_the_element_place() {
    let origins_of = |call: &str| {
        let (id, _) = only_fact(|f| {
            matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
        });
        let mut origins: Vec<_> = routes_to(id)
            .iter()
            .map(|r| format!("{:?}", r.origin))
            .collect();
        origins.sort();
        origins
    };
    let mut both = vec![
        format!("{:?}", item(OS, "OsBackups")),
        format!("{:?}", item(REGISTRY, "quietBackups")),
    ];
    both.sort();
    assert_eq!(origins_of("primary.write(\"primary\")"), both);

    // `second` has a default, so the element read and the default are each conditional.
    let into_second: Vec<_> =
        facts_where(|f| f.object == place(REGISTRY, "writePrimary", "second"));
    assert_eq!(into_second.len(), 2, "{into_second:#?}");
    let hops: Vec<_> = graph()
        .routes
        .values()
        .flat_map(|r| &r.hops)
        .filter(|h| h.to == place(REGISTRY, "writePrimary", "second"))
        .map(|h| h.support.clone())
        .collect();
    assert!(hops.contains(&Support::Possible {
        condition: Some("[1] !== undefined".into())
    }));
    assert!(hops.contains(&Support::Possible {
        condition: Some("[1] === undefined".into())
    }));
}

const CALLBACKS: &str = "@sample/app/src/callbacks.ts";

/// The route from `new OsBackups()` inside withBackups to a write call in
/// the function given to it.
fn callback_route(call: &str) -> &'static Route {
    let (id, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
    });
    let routes = routes_to(id);
    assert_eq!(routes.len(), 1, "{call}: {routes:#?}");
    routes[0]
}

#[test]
fn a_closure_given_as_a_callback_receives_the_callers_argument() {
    let route = callback_route("backups.write(\"inline\")");
    assert_eq!(route.origin, item(OS, "OsBackups"));
    assert_eq!(route.end, RouteEnd::ReachedUse);
    let slot = place(CALLBACKS, "withBackups", "use(0)");
    assert_eq!(route.hops[0].to, slot);
    assert_eq!(route.hops[0].support, Support::Possible { condition: None });
    assert_eq!(route.hops[1].support, Support::Possible { condition: None });
}

#[test]
fn a_named_function_given_as_a_callback_receives_the_callers_argument() {
    let route = callback_route("backups.write(\"named\")");
    assert_eq!(route.origin, item(OS, "OsBackups"));
    assert_eq!(route.hops[1].to, place(CALLBACKS, "label", "backups"));
}

#[test]
fn a_callbacks_returned_value_comes_back_through_its_result() {
    let into_result = facts_where(|f| f.object == place(CALLBACKS, "withBackups", "use return"));
    // The closure's body and the named function's return.
    assert_eq!(into_result.len(), 2, "{into_result:#?}");
    let out = facts_where(|f| f.subject == place(CALLBACKS, "withBackups", "use return"));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1.object, place(CALLBACKS, "withBackups", "return"));
}

#[test]
fn a_callback_parameter_typed_as_a_project_function_still_fills_its_slots() {
    // Into the closure's parameter.
    let into = callback_route("given.prune(5)");
    assert_eq!(into.origin, item(OS, "OsBackups"));
    assert_eq!(into.hops[0].to, place(CALLBACKS, "withTyped", "use(0)"));
    // And back out through the callback's result.
    let (id, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_))
            && f.site
                .as_ref()
                .is_some_and(|s| text(s).ends_with(".write(\"typed\")"))
    });
    let out = routes_to(id);
    assert_eq!(out.len(), 1, "{out:#?}");
    assert_eq!(out[0].origin, item(OS, "OsBackups"));
    assert!(
        out[0]
            .hops
            .iter()
            .any(|h| h.to == place(CALLBACKS, "withTyped", "use return"))
    );
    // The call through `use` is indirect, not a call of sameBackups.
    assert!(
        facts_where(|f| {
            f.subject == item(CALLBACKS, "withTyped") && f.object == item(CALLBACKS, "sameBackups")
        })
        .is_empty()
    );
}

/// The origins of the value routes for the call whose source text is `call`, sorted.
fn sorted_origins(call: &str) -> Vec<String> {
    let (id, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_)) && f.site.as_ref().is_some_and(|s| text(s) == call)
    });
    let mut origins: Vec<_> = routes_to(id)
        .iter()
        .map(|r| format!("{:?}", r.origin))
        .collect();
    origins.sort();
    origins
}

/// The supports of every route hop into `to`.
fn supports_into(to: &Endpoint) -> Vec<Support> {
    graph()
        .routes
        .values()
        .flat_map(|r| &r.hops)
        .filter(|h| &h.to == to)
        .map(|h| h.support.clone())
        .collect()
}

#[test]
fn an_object_pattern_in_an_assignment_reads_the_fields_it_names() {
    assert_eq!(
        sorted_origins("picked.write(\"reassigned\")"),
        [format!("{:?}", item(OS, "OsBackups"))]
    );
    let read = only_fact(|f| f.object == place(REGISTRY, "reassign", "picked")).1;
    assert_eq!(read.kind, FactKind::Read);
    assert_eq!(read.subject, place(REGISTRY, "Holder", "backups"));

    // `spare = quietBackups`: the field read and the default are each conditional.
    let spare = place(REGISTRY, "reassign", "spare");
    assert_eq!(facts_where(|f| f.object == spare).len(), 2);
    assert!(supports_into(&spare).contains(&Support::Possible {
        condition: Some("spare === undefined".into())
    }));
}

#[test]
fn an_array_pattern_in_an_assignment_reads_the_element_place() {
    let mut both = vec![
        format!("{:?}", item(OS, "OsBackups")),
        format!("{:?}", item(REGISTRY, "quietBackups")),
    ];
    both.sort();
    assert_eq!(sorted_origins("first.write(\"in order\")"), both);

    // `second = new FallbackBackups()` is a default inside the pattern, not a
    // plain assignment.
    let second = place(REGISTRY, "reassign", "second");
    assert_eq!(facts_where(|f| f.object == second).len(), 2);
    let supports = supports_into(&second);
    assert!(supports.contains(&Support::Possible {
        condition: Some("[1] !== undefined".into())
    }));
    assert!(supports.contains(&Support::Possible {
        condition: Some("[1] === undefined".into())
    }));
    assert!(!supports.contains(&Support::Established));
}

#[test]
fn a_shorthand_pattern_reassigns_a_name_bound_by_destructuring() {
    let backups = place(REGISTRY, "rebind", "backups");
    let into: Vec<_> = facts_where(|f| f.object == backups)
        .into_iter()
        .map(|(_, f)| (f.kind.clone(), f.subject.clone()))
        .collect();
    let field = place(REGISTRY, "Holder", "backups");
    // One read when declared, one when reassigned.
    assert_eq!(
        into,
        [(FactKind::Read, field.clone()), (FactKind::Read, field)],
        "{into:#?}"
    );
}

#[test]
fn an_assignment_inside_a_shorthand_default_is_recorded() {
    let late = place(REGISTRY, "lateFallback", "late");
    let quiet = Endpoint::Item(id(REGISTRY, "quietBackups"));
    let stores = facts_where(|f| f.object == late && f.subject == quiet);
    assert_eq!(stores.len(), 1, "{stores:#?}");
}

const PATHS: &str = "@sample/backup-os/src/paths.ts";

#[test]
fn namespace_members_are_items_visible_when_exported() {
    let items = &graph().items;
    let check = |name: &str, kind: ItemKind, visibility: Visibility| {
        let found = &items[&id(PATHS, name)];
        assert_eq!(
            (&found.kind, &found.visibility),
            (&kind, &visibility),
            "{name}"
        );
    };
    check("Paths", ItemKind::Module, Visibility::Exported);
    check("Paths.folder", ItemKind::Function, Visibility::Exported);
    check("Paths.root", ItemKind::Function, Visibility::Internal);
    check("Paths.Folder", ItemKind::Type, Visibility::Exported);
    check("Paths.Folder.path", ItemKind::Method, Visibility::Exported);
    // `namespace Limits.Disk` nests Disk in Limits.
    check("Paths.Limits", ItemKind::Module, Visibility::Exported);
    check("Paths.Limits.Disk", ItemKind::Module, Visibility::Exported);
    check(
        "Paths.Limits.Disk.quota",
        ItemKind::Value,
        Visibility::Exported,
    );
    assert_eq!(
        items[&id(PATHS, "Paths")].signature.as_deref(),
        Some("export namespace Paths")
    );
    // A class merged with a namespace: the namespace's own rules decide its members.
    check("Shelf", ItemKind::Type, Visibility::Exported);
    check("Shelf.slots", ItemKind::Value, Visibility::Exported);
    check("Shelf.label", ItemKind::Function, Visibility::Internal);
    check("Shelf.size", ItemKind::Method, Visibility::Exported);
}

#[test]
fn a_value_exported_from_a_namespace_is_an_origin() {
    let (call, _) = only_fact(|f| {
        matches!(f.kind, FactKind::Call(_))
            && f.site
                .as_ref()
                .is_some_and(|s| text(s) == "Paths.defaultBackups.write(\"default\")")
    });
    let routes = routes_to(call);
    assert_eq!(routes.len(), 1, "{routes:#?}");
    // A top-level value is an origin, like quietBackups; inside a namespace too.
    assert_eq!(routes[0].origin, item(PATHS, "Paths.defaultBackups"));
    assert_eq!(routes[0].end, RouteEnd::ReachedUse);
    // Its own value comes from the constructor.
    only_fact(|f| {
        f.kind == FactKind::Construct
            && f.subject == item(OS, "OsBackups")
            && f.object == item(PATHS, "Paths.defaultBackups")
    });
}

#[test]
fn calls_into_a_namespace_reach_its_members() {
    let where_ = item("@sample/app/src/paths.ts", "where");
    let call = |callee: Endpoint| {
        only_fact(|f| {
            f.subject == where_ && f.object == callee && matches!(f.kind, FactKind::Call(_))
        })
    };
    let (folder, fact) = call(item(PATHS, "Paths.folder"));
    assert_eq!(fact.kind, FactKind::Call(Dispatch::Direct));
    // A call into another package is a direct-use route.
    assert!(
        graph()
            .routes
            .contains_key(&RouteId(format!("use:{}", folder.0)))
    );
    let (path, _) = call(item(PATHS, "Paths.Folder.path"));
    let routes = routes_to(path);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].origin, item(PATHS, "Paths.Folder"));
    // Inside the namespace, the class's method calls the namespace's function.
    only_fact(|f| {
        f.subject == item(PATHS, "Paths.Folder.path") && f.object == item(PATHS, "Paths.folder")
    });
}
