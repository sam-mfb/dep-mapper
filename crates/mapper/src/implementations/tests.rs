use super::*;

#[test]
fn a_plain_workbook_path_is_used_as_is() {
    assert_eq!(
        eval_command(Path::new("/tmp/srs-sample/work_book.1")),
        "srs eval --workbook /tmp/srs-sample/work_book.1"
    );
}

#[test]
fn other_paths_are_quoted_for_the_shell() {
    assert_eq!(
        eval_command(Path::new("/tmp/my work")),
        "srs eval --workbook '/tmp/my work'"
    );
    assert_eq!(
        eval_command(Path::new("/tmp/it's $HOME")),
        r"srs eval --workbook '/tmp/it'\''s $HOME'"
    );
}

fn selection() -> BuildSelection {
    BuildSelection {
        root: "/src/project".into(),
        target: "lib".into(),
        configuration: "default features".into(),
    }
}

#[test]
fn the_llm_evaluator_needs_an_api_key() {
    let installed = Installed {
        anthropic_api_key: None,
    };
    let Err(error) = installed.evaluator(&EvaluatorChoice::Llm, &selection(), &LayerPlan::Proposed)
    else {
        panic!("no evaluator without a key")
    };
    assert!(error.0.contains("ANTHROPIC_API_KEY"));
}

#[test]
fn with_a_key_the_llm_evaluator_is_supplied_without_contacting_the_api() {
    let installed = Installed {
        anthropic_api_key: Some("test-key".into()),
    };
    assert!(
        installed
            .evaluator(&EvaluatorChoice::Llm, &selection(), &LayerPlan::Proposed)
            .is_ok()
    );
}

#[test]
fn an_invalid_fixed_layer_list_is_refused_before_any_evaluator_is_built() {
    let installed = Installed {
        anthropic_api_key: Some("test-key".into()),
    };
    for layers in [vec![], vec!["top".to_string(), " ".to_string()]] {
        let plan = LayerPlan::Fixed {
            layers,
            previous: BTreeMap::new(),
        };
        let Err(error) = installed.evaluator(&EvaluatorChoice::Llm, &selection(), &plan) else {
            panic!("an invalid list is refused")
        };
        assert!(
            error.0.starts_with("the layers cannot be fixed"),
            "{}",
            error.0
        );
    }
}
