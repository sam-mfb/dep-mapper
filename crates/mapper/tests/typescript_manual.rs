//! `srs map` on the TypeScript grapher's sample workspace with the manual
//! evaluator, as a person runs it: the first run writes the workbook, `srs
//! eval` records answers and submits them, and the second run saves the map
//! and its page. Needs Node and `npm ci` in the TypeScript collector, like
//! that crate's own tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn srs(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_srs"))
        .args(args)
        .output()
        .expect("srs runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn sample() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../project-graph-typescript/sample")
        .display()
        .to_string()
}

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typescript_manual");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("old scratch directory removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch directory created");
    dir
}

#[test]
fn a_typescript_project_is_mapped_after_its_workbook_is_answered() {
    let dir = scratch();
    let workbook = dir.join("workbook").display().to_string();
    let out = dir.join("map.toml");
    let page = dir.join("map.html");
    let map_args = [
        "map",
        &sample(),
        "--language",
        "typescript",
        "--target",
        "tsconfig.json",
        "--configuration",
        "tsconfig",
        "--evaluator",
        "manual",
        "--workbook",
        &workbook,
        "--out",
        &out.display().to_string(),
        "--page",
        &page.display().to_string(),
    ];

    let first = srs(&map_args);
    assert_eq!(first.status.code(), Some(1), "{}", text(&first.stderr));
    assert!(text(&first.stderr).contains("were not submitted"));
    assert!(!out.exists());

    let eval = |args: &[&str]| {
        let output = srs(&[&["eval", "--workbook", &workbook], args].concat());
        assert!(output.status.success(), "{}", text(&output.stdout));
        text(&output.stdout)
    };
    assert!(eval(&["index"]).contains("module/@sample/installer"));
    eval(&["clusters", "Program", "Core"]);
    eval(&[
        "judge",
        "module:@sample/installer",
        "--concern",
        "Installs tools, saving backups through the Backups contract",
        "--kind",
        "logic",
        "--cluster",
        "Core",
        "--reason",
        "It calls Backups.write on whatever it is given",
        "--cite",
        "route:value:call@packages/installer/src/index.ts:227:0",
    ]);
    eval(&[
        "relate",
        "routes/@sample/backup-api+@sample/installer",
        "--kind",
        "direct-use",
        "--reason",
        "installer calls the contract",
        "--cite",
        "route:use:call@packages/installer/src/index.ts:227",
    ]);
    eval(&["submit"]);

    let second = srs(&map_args);
    assert!(second.status.success(), "{}", text(&second.stderr));
    let saved = std::fs::read_to_string(&out).expect("the map is saved");
    let map_persist::MapDocument::Map(map) = map_persist::load(&saved).expect("the map loads")
    else {
        panic!("a map, not a draft")
    };
    assert_eq!(map.problems(), vec![]);
    assert_eq!(map.project, "backup-sample");
    let installer = &map.modules[&project_map::ModuleId("@sample/installer".into())];
    let project_map::Review::Judged { judgment, .. } = &installer.description else {
        panic!("the installer is judged")
    };
    assert_eq!(judgment.layer.as_ref().map(|l| l.0.as_str()), Some("Core"));
    assert!(
        std::fs::read_to_string(&page)
            .expect("the page is written")
            .contains("backup-sample")
    );
}

#[test]
fn an_answer_the_checks_refuse_exits_with_status_1() {
    let dir = scratch().join("refused");
    let workbook = dir.join("workbook").display().to_string();
    srs(&[
        "map",
        &sample(),
        "--language",
        "typescript",
        "--target",
        "tsconfig.json",
        "--configuration",
        "tsconfig",
        "--evaluator",
        "manual",
        "--workbook",
        &workbook,
        "--out",
        &dir.join("map.toml").display().to_string(),
    ]);
    let refused = srs(&[
        "eval",
        "--workbook",
        &workbook,
        "judge",
        "module:@sample/installer",
        "--concern",
        "c",
        "--kind",
        "logic",
        "--cluster",
        "Unlisted",
        "--reason",
        "r",
        "--cite",
        "route:no-such-route",
    ]);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused.stdout));
    let missing = srs(&[
        "eval",
        "--workbook",
        &dir.join("none").display().to_string(),
        "index",
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing.stderr).contains("there is no workbook"));
}
