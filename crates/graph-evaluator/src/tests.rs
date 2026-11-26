use super::*;
use crate::testing::FixedEvaluator;
use project_graph::{
    Build, BuildUnit, Cited, Endpoint, Fact, FactId, FactKind, Hop, Module, Route, RouteEnd,
    RouteId, Support, UnitId,
};

#[test]
fn evaluation_is_scoped_to_one_snapshot_and_interprets_one_of_two_routes() {
    let module = ModuleId("kashkeshet".into());
    let claude = ModuleId("kkt-claude".into());
    let unit = UnitId("kkt".into());
    let mut graph = ProjectGraph {
        id: GraphId("kkt/linux:fixture".into()),
        build: Build {
            project: "kashkeshet".into(),
            source_revision: "41fbc289".into(),
            target: "kkt".into(),
            configuration: "linux, default features".into(),
        },
        modules: BTreeMap::from([
            (
                module.clone(),
                Module {
                    name: "kashkeshet".into(),
                    root: ".".into(),
                },
            ),
            (
                claude.clone(),
                Module {
                    name: "kkt-claude".into(),
                    root: "crates/kkt-claude".into(),
                },
            ),
        ]),
        items: BTreeMap::new(),
        units: BTreeMap::from([(
            unit.clone(),
            BuildUnit {
                module: module.clone(),
                target: "kkt".into(),
                settings: BTreeMap::new(),
            },
        )]),
        routes: BTreeMap::new(),
        facts: BTreeMap::new(),
        gaps: vec![],
    };
    let agent = RouteId("ClaudeCommand/add_server via agent parts".into());
    let install = RouteId("ClaudeCommand/add_server via install parts".into());
    for (route, site, file, bytes) in [
        (
            &agent,
            "agent_parts",
            "src/machine/agent_parts.rs",
            1415..1454,
        ),
        (
            &install,
            "install_parts",
            "src/machine/native.rs",
            3678..3730,
        ),
    ] {
        let origin = Endpoint::Module(module.clone());
        let parts = project_graph::ItemId(site.into());
        graph.items.insert(
            parts.clone(),
            project_graph::Item {
                module: module.clone(),
                name: site.into(),
                kind: project_graph::ItemKind::Function,
                visibility: project_graph::Visibility::Internal,
                signature: None,
                declaration: None,
            },
        );
        let field = Endpoint::Place {
            owner: parts,
            projection: "client_command".into(),
        };
        let fact = FactId(format!("{site}.client_command"));
        graph.facts.insert(
            fact.clone(),
            Fact {
                unit: unit.clone(),
                subject: origin.clone(),
                kind: FactKind::Store,
                object: field.clone(),
                site: Some(project_graph::SourceSpan {
                    file: file.into(),
                    bytes,
                }),
            },
        );
        graph.routes.insert(
            route.clone(),
            Route {
                origin,
                use_sites: Vec::new(),
                hops: vec![Hop {
                    to: field,
                    facts: vec![fact.clone()],
                    support: Support::Established,
                }],
                end: RouteEnd::Stopped {
                    at: vec![fact],
                    reason: "Partial fixture ends at the supply site.".into(),
                },
            },
        );
    }
    let evaluation = GraphEvaluation {
        graph: graph.id.clone(),
        clusters: Clusters::empty(),
        modules: BTreeMap::new(),
        relationships: vec![Relationship {
            kind: RelationshipKind::Wires,
            reason: "The program supplies the client command in the agent parts field.".into(),
            evidence: vec![
                GraphRef::from(agent.clone()),
                GraphRef::from(FactId("agent_parts.client_command".into())),
            ],
        }],
    };
    let fixed = FixedEvaluator(evaluation);
    let evaluator: &dyn GraphEvaluator = &fixed;
    let answer = evaluator.evaluate(&graph).unwrap();
    assert!(matches!(
        graph.resolve(&answer.relationships[0].evidence[0]),
        Some(Cited::Route(found)) if found == &graph.routes[&agent]
    ));
    let another_build = ProjectGraph {
        id: GraphId("kkt/other:fixture".into()),
        ..graph
    };
    assert!(evaluator.evaluate(&another_build).is_err());
}
