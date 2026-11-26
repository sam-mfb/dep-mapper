use super::*;
use crate::collection::records::parse;
use project_graph::RouteId;

fn selected() -> Selected {
    Selected {
        source_revision: "abc".into(),
        target: "tsconfig.json".into(),
        configuration: "tsconfig".into(),
    }
}

fn records(lines: &[&str]) -> Vec<Record> {
    lines.iter().map(|l| parse(l).unwrap()).collect()
}

const PROJECT: &str =
    r#"{"record":"project","name":"demo","settings":{"strict":"true"},"sources":"00"}"#;
const MODULE: &str = r#"{"record":"module","id":"app","name":"app","root":"packages/app"}"#;
const END: &str = r#"{"record":"end"}"#;

#[test]
fn the_graph_names_the_project_revision_and_selection() {
    let graph = assemble(
        &selected(),
        records(&[PROJECT, MODULE, END]),
        TraceLimits::STANDARD,
        &[],
    )
    .unwrap();
    assert!(
        graph
            .id
            .0
            .starts_with("typescript:demo@abc:tsconfig.json:tsconfig:"),
        "{}",
        graph.id.0
    );
    assert_eq!(graph.build.project, "demo");
    assert_eq!(graph.build.source_revision, "abc");
    let unit = &graph.units[&UnitId("app:tsconfig.json".into())];
    assert_eq!(unit.module, ModuleId("app".into()));
    assert_eq!(unit.settings["strict"], "true");
}

#[test]
fn output_without_the_end_record_is_incomplete() {
    let error = assemble(
        &selected(),
        records(&[PROJECT, MODULE]),
        TraceLimits::STANDARD,
        &[],
    )
    .unwrap_err();
    assert_eq!(error, AssemblyError::Incomplete);
}

#[test]
fn output_without_a_project_record_is_refused() {
    let error = assemble(
        &selected(),
        records(&[MODULE, END]),
        TraceLimits::STANDARD,
        &[],
    )
    .unwrap_err();
    assert_eq!(error, AssemblyError::NoProject);
}

#[test]
fn a_fact_in_an_unlisted_module_is_refused() {
    let fact = r#"{"record":"fact","id":"f","module":"ghost","subject":{"module":"ghost"},"fact":{"kind":"declared_dependency"},"object":{"external":"x"},"site":null,"condition":null,"link":null,"joins_instances":false}"#;
    let error = assemble(
        &selected(),
        records(&[PROJECT, MODULE, fact, END]),
        TraceLimits::STANDARD,
        &[],
    )
    .unwrap_err();
    assert_eq!(error, AssemblyError::UnknownModule("ghost".into()));
}

#[test]
fn facts_join_their_package_unit_and_conditions_mark_hops_possible() {
    let lines = [
        PROJECT,
        MODULE,
        r#"{"record":"item","id":"app/a.ts#C","module":"app","name":"C","kind":"type","parent":null,"visibility":"internal","signature":"class C","declaration":{"file":"packages/app/a.ts","start":0,"end":9}}"#,
        r#"{"record":"item","id":"app/a.ts#C.go","module":"app","name":"C.go","kind":"method","parent":"app/a.ts#C","visibility":"internal","signature":null,"declaration":null}"#,
        r#"{"record":"fact","id":"new","module":"app","subject":{"item":"app/a.ts#C"},"fact":{"kind":"construct"},"object":{"place":{"owner":"app/a.ts#f","projection":"c"}},"site":{"file":"packages/app/a.ts","start":20,"end":27},"condition":"flag","link":null,"joins_instances":false}"#,
        r#"{"record":"fact","id":"recv","module":"app","subject":{"place":{"owner":"app/a.ts#f","projection":"c"}},"fact":{"kind":"pass"},"object":{"place":{"owner":"app/a.ts#C.go","projection":"self"}},"site":null,"condition":null,"link":null,"joins_instances":false}"#,
        r#"{"record":"fact","id":"call","module":"app","subject":{"item":"app/a.ts#f"},"fact":{"kind":"call","dispatch":"direct"},"object":{"item":"app/a.ts#C.go"},"site":null,"condition":null,"link":null,"joins_instances":false}"#,
        r#"{"record":"receiver","call":"call","moves":["recv"]}"#,
        r#"{"record":"gap","module":"app","site":null,"reason":"TypeScript error: oops"}"#,
        END,
    ];
    let graph = assemble(&selected(), records(&lines), TraceLimits::STANDARD, &[]).unwrap();
    let item = &graph.items[&ItemId("app/a.ts#C".into())];
    assert_eq!(item.kind, ItemKind::Type);
    assert_eq!(item.declaration.as_ref().unwrap().bytes, 0..9);
    assert_eq!(
        graph.facts[&FactId("new".into())].unit,
        UnitId("app:tsconfig.json".into())
    );
    let route = &graph.routes[&RouteId("value:call:0".into())];
    assert_eq!(route.origin, Endpoint::Item(ItemId("app/a.ts#C".into())));
    assert_eq!(
        route.hops[0].support,
        project_graph::Support::Possible {
            condition: Some("flag".into())
        }
    );
    assert_eq!(graph.gaps[0].unit, Some(UnitId("app:tsconfig.json".into())));
    assert_eq!(graph.gaps[0].reason, "TypeScript error: oops");
}

#[test]
fn the_graph_id_changes_with_the_contents_and_only_with_them() {
    let item = |name: &str| {
        format!(
            r#"{{"record":"item","id":"app/a.ts#{name}","module":"app","name":"{name}","kind":"value","parent":null,"visibility":"internal","signature":null,"declaration":null}}"#
        )
    };
    let build = |name: &str| {
        let lines = [
            PROJECT.to_string(),
            MODULE.to_string(),
            item(name),
            END.to_string(),
        ];
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        assemble(&selected(), records(&lines), TraceLimits::STANDARD, &[])
            .unwrap()
            .id
    };
    assert_eq!(build("probeOne"), build("probeOne"));
    assert_ne!(build("probeOne"), build("probeTwo"));
}

/// `app` declares a dependency on `lodash` and calls `lodash#chunk` and
/// `clock#now`.
fn outside_uses() -> Vec<&'static str> {
    vec![
        PROJECT,
        MODULE,
        r#"{"record":"item","id":"app/a.ts#f","module":"app","name":"f","kind":"function","parent":null,"visibility":"internal","signature":null,"declaration":null}"#,
        r#"{"record":"fact","id":"dep","module":"app","subject":{"module":"app"},"fact":{"kind":"declared_dependency"},"object":{"external":"lodash"},"site":null,"condition":null,"link":null,"joins_instances":false}"#,
        r#"{"record":"fact","id":"chunk","module":"app","subject":{"item":"app/a.ts#f"},"fact":{"kind":"call","dispatch":"foreign"},"object":{"external":"lodash#chunk"},"site":null,"condition":null,"link":null,"joins_instances":false}"#,
        r#"{"record":"fact","id":"now","module":"app","subject":{"item":"app/a.ts#f"},"fact":{"kind":"call","dispatch":"foreign"},"object":{"external":"clock#now"},"site":null,"condition":null,"link":null,"joins_instances":false}"#,
        END,
    ]
}

fn outside_origins(graph: &ProjectGraph) -> Vec<&str> {
    let mut names: Vec<&str> = graph
        .routes
        .values()
        .filter_map(|r| match &r.origin {
            Endpoint::External(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    names.sort();
    names
}

#[test]
fn without_exclusions_every_outside_route_stays_and_nothing_is_recorded() {
    let graph = assemble(
        &selected(),
        records(&outside_uses()),
        TraceLimits::STANDARD,
        &[],
    )
    .unwrap();
    assert_eq!(
        outside_origins(&graph),
        ["clock#now", "lodash", "lodash#chunk"]
    );
    assert!(graph.gaps.is_empty());
}

#[test]
fn an_excluded_package_loses_its_routes_and_facts_and_the_graph_says_so() {
    let graph = assemble(
        &selected(),
        records(&outside_uses()),
        TraceLimits::STANDARD,
        &["lodash".to_string()],
    )
    .unwrap();
    assert_eq!(outside_origins(&graph), ["clock#now"]);
    assert_eq!(
        graph.facts.keys().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        ["now"]
    );
    assert_eq!(
        graph.gaps[0].reason,
        "Routes that start in the excluded packages lodash are left out: \
         2 routes and 2 facts removed."
    );
}

#[test]
fn the_graph_id_covers_the_exclusions() {
    let id = |excluded: &[String]| {
        assemble(
            &selected(),
            records(&outside_uses()),
            TraceLimits::STANDARD,
            excluded,
        )
        .unwrap()
        .id
    };
    assert_ne!(id(&[]), id(&["lodash".to_string()]));
}

#[test]
fn a_package_matches_the_part_of_a_name_before_its_hash() {
    let packages = ["@scope/pkg".to_string()];
    assert!(in_packages(&packages, "@scope/pkg#Thing.method"));
    assert!(in_packages(&packages, "@scope/pkg"));
    assert!(!in_packages(&packages, "@scope/pkg-extra#Thing"));
    assert!(!in_packages(&packages, "other#@scope/pkg"));
}
