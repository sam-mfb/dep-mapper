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
            language: Language::TypeScript { excluded: vec![] },
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

#[test]
fn the_rust_cache_defaults_to_the_projects_target_directory() {
    let language = |extra: &[&str]| {
        let args = [&["--evaluator", "llm"], extra].concat();
        let Command::Map(command) = parse(map_args(&args)).unwrap() else {
            panic!("a map command")
        };
        command.language
    };
    assert_eq!(
        language(&[]),
        Language::Rust {
            cache: "/home/someone/work/./target/srs".into(),
            excluded: rust_default(),
        }
    );
    assert_eq!(
        language(&["--cache", "cache"]),
        Language::Rust {
            cache: "/home/someone/work/cache".into(),
            excluded: rust_default(),
        }
    );
}

#[test]
fn a_cache_for_typescript_is_a_mistake() {
    let outcome = parse([
        "srs",
        "map",
        ".",
        "--language",
        "typescript",
        "--target",
        "tsconfig.json",
        "--configuration",
        "tsconfig",
        "--evaluator",
        "llm",
        "--out",
        "m",
        "--cache",
        "c",
    ])
    .unwrap_err();
    assert!(outcome.text.contains("--cache applies only"));
}

fn rust_default() -> Vec<String> {
    [
        "std",
        "core",
        "alloc",
        "thiserror",
        "futures",
        "serde",
        "serde_json",
        "tokio",
        "tokio-util",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect()
}

fn excluded(extra: &[&str]) -> Vec<String> {
    let args = [&["--evaluator", "llm"], extra].concat();
    let Command::Map(command) = parse(map_args(&args)).unwrap() else {
        panic!("a map command")
    };
    match command.language {
        Language::Rust { excluded, .. } | Language::TypeScript { excluded } => excluded,
    }
}

#[test]
fn rust_leaves_out_the_toolchain_and_the_crates_that_shape_the_language_by_default() {
    assert_eq!(excluded(&[]), rust_default());
}

#[test]
fn the_exclude_flag_replaces_the_default_list() {
    assert_eq!(
        excluded(&["--exclude", "std, clap,,anyhow"]),
        ["std", "clap", "anyhow"]
    );
}

#[test]
fn an_empty_exclude_flag_keeps_everything() {
    assert!(excluded(&["--exclude", ""]).is_empty());
}

#[test]
fn typescript_excludes_nothing_by_default_and_takes_the_flag() {
    let typescript = |extra: &[&str]| {
        let mut args = vec![
            "srs",
            "map",
            ".",
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
        ];
        args.extend(extra);
        let Command::Map(command) = parse(args).unwrap() else {
            panic!("a map command")
        };
        command.language
    };
    assert_eq!(typescript(&[]), Language::TypeScript { excluded: vec![] });
    assert_eq!(
        typescript(&["--exclude", "@types/node"]),
        Language::TypeScript {
            excluded: vec!["@types/node".into()]
        }
    );
}
