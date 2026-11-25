//! `srs map` on the TypeScript grapher's sample workspace with the manual
//! evaluator, as a person runs it: the first run writes the workbook, `srs
//! eval` records answers and submits them, and the second run saves the map
//! and its page. Needs Node and `npm ci` in the TypeScript collector, like
//! that crate's own tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn srs<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Output {
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

/// An empty directory of its own for each test, as tests run in parallel.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("typescript_manual")
        .join(test);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("old scratch directory removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch directory created");
    dir
}

/// Records a few answers in the workbook and submits them, as a person would.
fn answer(workbook: &str) {
    let eval = |args: &[&str]| {
        let output = srs(&[&["eval", "--workbook", workbook], args].concat());
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
}

fn map_args(workbook: &str, out: &Path, extra: &[&str]) -> Vec<String> {
    let sample = sample();
    let base = [
        "map",
        sample.as_str(),
        "--language",
        "typescript",
        "--target",
        "tsconfig.json",
        "--configuration",
        "tsconfig",
        "--evaluator",
        "manual",
        "--workbook",
        workbook,
        "--out",
        &out.display().to_string(),
    ];
    base.iter()
        .chain(extra)
        .map(|arg| arg.to_string())
        .collect()
}

#[test]
fn with_agent_wait_one_run_waits_for_the_answers() {
    let dir = scratch("wait");
    let workbook = dir.join("workbook").display().to_string();
    let out = dir.join("map.toml");
    let child = Command::new(env!("CARGO_BIN_EXE_srs"))
        .args(map_args(&workbook, &out, &["--agent", "wait"]))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("srs starts");
    // The workbook exists once the request is shown.
    let index = Path::new(&workbook).join("workbook.json");
    for _ in 0..600 {
        if index.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    answer(&workbook);
    let output = child.wait_with_output().expect("srs finishes");
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stderr).contains("Waiting for the answers to be submitted"));
    assert!(out.exists());
}

#[test]
fn a_typescript_project_is_mapped_after_its_workbook_is_answered() {
    let dir = scratch("two-runs");
    let workbook = dir.join("workbook").display().to_string();
    let out = dir.join("map.toml");
    let page = dir.join("map.html");
    let map_args = map_args(&workbook, &out, &["--page", &page.display().to_string()]);

    let first = srs(&map_args);
    assert_eq!(first.status.code(), Some(1), "{}", text(&first.stderr));
    assert!(text(&first.stderr).contains("were not submitted"));
    assert!(!out.exists());

    answer(&workbook);

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
    let written = std::fs::read_to_string(&page).expect("the page is written");
    assert!(written.contains("backup-sample"));

    // The saved map reopens without analysis and shows the same page.
    let reopened = dir.join("reopened.html");
    let shown = srs(&[
        "show".to_string(),
        out.display().to_string(),
        "--page".into(),
        reopened.display().to_string(),
    ]);
    assert!(shown.status.success(), "{}", text(&shown.stderr));
    assert_eq!(
        std::fs::read_to_string(&reopened).expect("the page is written again"),
        written
    );
}

#[test]
fn an_answer_the_checks_refuse_exits_with_status_1() {
    let dir = scratch("refused");
    let workbook = dir.join("workbook").display().to_string();
    srs(&map_args(&workbook, &dir.join("map.toml"), &[]));
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
    // A reader that goes away, as `| head` does, ends the command quietly.
    let mut piped = Command::new(env!("CARGO_BIN_EXE_srs"))
        .args(["eval", "--workbook", &workbook, "index"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("srs starts");
    drop(piped.stdout.take());
    let closed = piped.wait_with_output().expect("srs finishes");
    assert!(closed.status.success(), "{}", text(&closed.stderr));
    assert_eq!(text(&closed.stderr), "");

    let missing = srs(&[
        "eval",
        "--workbook",
        &dir.join("none").display().to_string(),
        "index",
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing.stderr).contains("there is no workbook"));
}
