use super::*;

#[test]
fn map_arguments_become_a_map_command() {
    let command = parse([
        "srs",
        "map",
        "/src/project",
        "--language",
        "typescript",
        "--target",
        "tsconfig.json",
        "--configuration",
        "tsconfig",
        "--evaluator",
        "llm",
        "--page",
        "map.html",
        "--serve",
        "127.0.0.1:8080",
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Map(MapCommand {
            selection: BuildSelection {
                root: "/src/project".into(),
                target: "tsconfig.json".into(),
                configuration: "tsconfig".into(),
            },
            language: Language::TypeScript,
            evaluator: EvaluatorChoice::Llm,
            page: Some("map.html".into()),
            serve: Some("127.0.0.1:8080".parse().unwrap()),
        })
    );
}

#[test]
fn a_missing_choice_is_a_mistake() {
    let outcome = parse(["srs", "map", ".", "--language", "rust"]).unwrap_err();
    assert!(!outcome.requested);
    assert!(outcome.text.contains("--target"));
}

#[test]
fn help_is_a_request() {
    let outcome = parse(["srs", "--help"]).unwrap_err();
    assert!(outcome.requested);
    assert!(outcome.text.contains("map"));
}
