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

/// `app` declares `clock` twice under different names, `serde` and the
/// workspace member `lib`.
fn declared() -> Workspace {
    let dependency = |package: &str, member: bool| Dependency {
        package: package.into(),
        member,
    };
    Workspace {
        root: "/work".into(),
        members: vec![Member {
            name: "app".into(),
            root: "app".into(),
            dependencies: [
                ("clock".to_string(), dependency("clock", false)),
                ("timer".to_string(), dependency("clock", false)),
                ("serde".to_string(), dependency("serde", false)),
                ("lib".to_string(), dependency("lib", true)),
            ]
            .into(),
        }],
        third_party: BTreeMap::new(),
        depends_on: BTreeMap::new(),
    }
}

fn depends(workspace: &mut Workspace, package: &str, on: &[&str]) {
    workspace
        .depends_on
        .insert(package.into(), on.iter().map(|p| p.to_string()).collect());
}

#[test]
fn a_direct_dependency_is_named_for_itself_and_one_reaching_another_names_it() {
    let mut workspace = declared();
    depends(&mut workspace, "clock", &["tick"]);
    assert_eq!(
        workspace.reached_through("app", "clock"),
        Some("clock".into())
    );
    assert_eq!(
        workspace.reached_through("app", "tick"),
        Some("clock".into()),
        "two names for one package are one candidate"
    );
    assert_eq!(workspace.reached_through("app", "elsewhere"), None);
}

#[test]
fn workspace_members_are_not_dependencies_to_name_through() {
    let mut workspace = declared();
    depends(&mut workspace, "lib", &["tick"]);
    assert_eq!(workspace.reached_through("app", "tick"), None);
    depends(&mut workspace, "clock", &["tick"]);
    assert_eq!(
        workspace.reached_through("app", "tick"),
        Some("clock".into())
    );
}

#[test]
fn when_several_reach_a_package_the_one_it_is_named_after_is_chosen() {
    let mut workspace = declared();
    depends(&mut workspace, "clock", &["serde_core", "sync_wrapper"]);
    depends(&mut workspace, "serde", &["serde_core", "sync_wrapper"]);
    assert_eq!(
        workspace.reached_through("app", "serde_core"),
        Some("serde".into())
    );
    // `sync_wrapper` is not named after `serde`, and nor is it after a
    // package whose name merely begins the same way.
    assert_eq!(workspace.reached_through("app", "sync_wrapper"), None);
    depends(&mut workspace, "clock", &["clockwork"]);
    depends(&mut workspace, "serde", &["clockwork"]);
    assert_eq!(workspace.reached_through("app", "clockwork"), None);
}

#[test]
fn two_versions_of_one_package_keep_the_dependencies_of_both() {
    let json = r#"{
        "workspace_root": "/work",
        "workspace_members": ["app"],
        "packages": [
            {"id": "app", "name": "app", "manifest_path": "/work/app/Cargo.toml", "targets": []},
            {"id": "w1", "name": "windows-sys", "manifest_path": "/r/w1/Cargo.toml", "targets": []},
            {"id": "w2", "name": "windows-sys", "manifest_path": "/r/w2/Cargo.toml", "targets": []},
            {"id": "t", "name": "windows-targets", "manifest_path": "/r/t/Cargo.toml", "targets": []},
            {"id": "l", "name": "windows-link", "manifest_path": "/r/l/Cargo.toml", "targets": []}
        ],
        "resolve": {"nodes": [
            {"id": "app", "deps": []},
            {"id": "w1", "deps": [{"name": "windows_targets", "pkg": "t"}]},
            {"id": "w2", "deps": [{"name": "windows_link", "pkg": "l"}]},
            {"id": "t", "deps": []},
            {"id": "l", "deps": []}
        ]}
    }"#;
    let workspace = parse(json).unwrap();
    assert_eq!(
        workspace.depends_on["windows-sys"],
        ["windows-link".to_string(), "windows-targets".to_string()].into()
    );
}
