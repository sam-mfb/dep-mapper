use super::*;

/// `cargo metadata` for the test fixture, trimmed to the fields read and with
/// the workspace moved to `/work`.
const FIXTURE: &str = include_str!("fixture.json");

#[test]
fn members_keep_their_directories_and_the_names_their_source_uses_for_dependencies() {
    let workspace = parse(FIXTURE).unwrap();
    assert_eq!(workspace.root, PathBuf::from("/work"));
    let consumer = workspace
        .members
        .iter()
        .find(|m| m.name == "consumer")
        .unwrap();
    assert_eq!(consumer.root, "consumer");
    assert_eq!(
        consumer.dependencies["store"],
        Dependency {
            package: "supplier".into(),
            member: true
        }
    );
    assert_eq!(
        consumer.dependencies["clock"],
        Dependency {
            package: "clock".into(),
            member: false
        }
    );
    assert_eq!(
        workspace
            .members
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["supplier", "consumer", "app"]
    );
}

#[test]
fn only_packages_outside_the_workspace_are_third_party() {
    let workspace = parse(FIXTURE).unwrap();
    assert_eq!(
        workspace.third_party,
        [("clock".to_string(), "clock".to_string())].into()
    );
}

#[test]
fn output_that_is_not_cargo_metadata_is_an_error() {
    assert!(parse("{}").unwrap_err().message.contains("workspace_root"));
    assert!(parse("not json").is_err());
}

#[test]
fn an_indirect_dependency_is_reached_through_the_one_direct_dependency_that_leads_to_it() {
    let mut workspace = parse(FIXTURE).unwrap();
    assert_eq!(
        workspace.reached_through("consumer", "clock"),
        Some("clock".into())
    );
    workspace
        .depends_on
        .insert("clock".into(), vec!["tick".into()]);
    assert_eq!(
        workspace.reached_through("consumer", "tick"),
        Some("clock".into())
    );
    // Two direct dependencies lead to it and neither names it: neither is chosen.
    workspace
        .depends_on
        .insert("supplier".into(), vec!["tick".into()]);
    assert_eq!(workspace.reached_through("consumer", "tick"), None);
    // Two lead to it and one names it, as `serde` does `serde_core`.
    workspace
        .depends_on
        .insert("clock".into(), vec!["clock_core".into()]);
    workspace
        .depends_on
        .insert("supplier".into(), vec!["clock_core".into()]);
    assert_eq!(
        workspace.reached_through("consumer", "clock_core"),
        Some("clock".into())
    );
    assert_eq!(workspace.reached_through("consumer", "elsewhere"), None);
}
