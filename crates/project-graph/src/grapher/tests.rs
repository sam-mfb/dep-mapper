use super::testing::FixedGrapher;
use super::*;
use crate::GraphId;
use std::collections::BTreeMap;

fn graph() -> ProjectGraph {
    ProjectGraph {
        id: GraphId("fixture".into()),
        build: Build {
            project: "fixture".into(),
            source_revision: "0".into(),
            target: "fixture-cli".into(),
            configuration: "default features".into(),
        },
        modules: BTreeMap::new(),
        units: BTreeMap::new(),
        items: BTreeMap::new(),
        facts: BTreeMap::new(),
        routes: BTreeMap::new(),
        gaps: vec![],
    }
}

fn selection(target: &str, configuration: &str) -> BuildSelection {
    BuildSelection {
        root: PathBuf::from("fixture"),
        target: target.into(),
        configuration: configuration.into(),
    }
}

#[test]
fn a_selection_describes_a_build_with_its_target_and_configuration() {
    let build = graph().build;
    assert!(selection("fixture-cli", "default features").describes(&build));
    assert!(!selection("fixture-lib", "default features").describes(&build));
    assert!(!selection("fixture-cli", "no default features").describes(&build));
}

#[test]
fn a_grapher_is_used_through_the_contract() {
    let grapher: &dyn ProjectGrapher = &FixedGrapher(graph());
    let built = grapher
        .build_graph(&selection("fixture-cli", "default features"))
        .expect("the selection matches the fixture");
    assert_eq!(built, graph());
}

#[test]
fn the_fixed_grapher_refuses_a_different_build() {
    let error = FixedGrapher(graph())
        .build_graph(&selection("fixture-lib", "default features"))
        .unwrap_err();
    assert_eq!(error.message, "fixture graph describes a different build");
}
