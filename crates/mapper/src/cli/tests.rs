use super::*;

const CWD: &str = "/home/someone/work";

fn parse<I, T>(args: I) -> Result<Command, ParseOutcome>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    super::parse(args, Path::new(CWD))
}

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
        "--out",
        "map.toml",
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
            out: "map.toml".into(),
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

fn map_args(extra: &[&str]) -> Vec<String> {
    let base = [
        "srs",
        "map",
        ".",
        "--language",
        "rust",
        "--target",
        "lib",
        "--configuration",
        "default features",
        "--out",
        "map.toml",
    ];
    base.iter()
        .chain(extra)
        .map(|arg| arg.to_string())
        .collect()
}

#[test]
fn the_manual_evaluator_takes_its_workbook() {
    let evaluator = |extra: &[&str]| {
        let args = [&["--evaluator", "manual", "--workbook", "wb"], extra].concat();
        let Command::Map(command) = parse(map_args(&args)).unwrap() else {
            panic!("a map command")
        };
        command.evaluator
    };
    let manual = |agent| EvaluatorChoice::Manual {
        workbook: "/home/someone/work/wb".into(),
        agent,
    };
    assert_eq!(evaluator(&[]), manual(Agent::None));
    assert_eq!(evaluator(&["--agent", "none"]), manual(Agent::None));
    assert_eq!(evaluator(&["--agent", "wait"]), manual(Agent::Wait));
    assert_eq!(evaluator(&["--agent", "claude"]), manual(Agent::Claude));
}

#[test]
fn the_manual_evaluator_without_a_workbook_is_a_mistake() {
    let outcome = parse(map_args(&["--evaluator", "manual"])).unwrap_err();
    assert!(!outcome.requested);
    assert!(outcome.text.contains("--workbook"));
}

#[test]
fn a_workbook_for_another_evaluator_is_a_mistake() {
    let outcome = parse(map_args(&["--evaluator", "llm", "--workbook", "wb"])).unwrap_err();
    assert!(!outcome.requested);
    assert!(outcome.text.contains("apply only to --evaluator manual"));
    let outcome = parse(map_args(&["--evaluator", "llm", "--agent", "wait"])).unwrap_err();
    assert!(outcome.text.contains("apply only to --evaluator manual"));
}

#[test]
fn eval_passes_its_arguments_through_unchanged() {
    let command = parse([
        "srs",
        "eval",
        "--workbook",
        "wb",
        "judge",
        "module:core",
        "--kind",
        "logic",
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Eval(EvalCommand {
            workbook: "/home/someone/work/wb".into(),
            arguments: ["judge", "module:core", "--kind", "logic"]
                .map(String::from)
                .to_vec(),
        })
    );
}

#[test]
fn an_absolute_workbook_path_is_kept() {
    let Command::Eval(command) = parse(["srs", "eval", "--workbook", "/data/wb", "index"]).unwrap()
    else {
        panic!("an eval command")
    };
    assert_eq!(command.workbook, Path::new("/data/wb"));
}
