//! `srs map` on sharsheret itself with the Rust grapher and the manual
//! evaluator, as two runs around `srs eval`. It maps the `map-builder`
//! package, whose dependencies are few. Needs the pinned toolchain with
//! `rustc-dev`, like project-graph-rust's own tests; the first run builds
//! the grapher's driver into the test's cache.

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

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn sharsheret_is_mapped_with_the_rust_grapher() {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sharsheret_rust");
    let dir = tmp.join("run");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("old run removed");
    }
    std::fs::create_dir_all(&dir).expect("run directory created");
    let workbook = dir.join("workbook").display().to_string();
    let out = dir.join("map.toml");
    let map_args = [
        "map".to_string(),
        repository().display().to_string(),
        "--language".into(),
        "rust".into(),
        "--target".into(),
        "map-builder".into(),
        "--configuration".into(),
        "default features".into(),
        "--cache".into(),
        // Kept between test runs, so the driver is built once.
        tmp.join("cache").display().to_string(),
        "--evaluator".into(),
        "manual".into(),
        "--workbook".into(),
        workbook.clone(),
        "--out".into(),
        out.display().to_string(),
    ];

    let first = srs(&map_args);
    assert_eq!(first.status.code(), Some(1), "{}", text(&first.stderr));
    assert!(text(&first.stderr).contains("were not submitted"));

    let eval = |args: &[&str]| {
        let output = srs(&[&["eval", "--workbook", workbook.as_str()], args].concat());
        assert!(output.status.success(), "{}", text(&output.stdout));
        text(&output.stdout)
    };
    assert!(eval(&["index"]).contains("module/map-builder"));
    eval(&["clusters", "Map"]);
    eval(&[
        "judge",
        "module:map-builder",
        "--concern",
        "Builds a project map from a graph and its evaluation",
        "--kind",
        "logic",
        "--cluster",
        "Map",
        "--reason",
        "Its one public function is pure",
        "--cite",
        "item:map-builder/lib:map_builder::build_map",
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
    let module = |name: &str| project_map::ModuleId(name.into());
    for name in [
        "map-builder",
        "project-graph",
        "graph-evaluator",
        "project-map",
    ] {
        assert!(
            map.modules.contains_key(&module(name)),
            "{name} is on the map"
        );
    }
    for used in ["project-graph", "graph-evaluator", "project-map"] {
        let dependency = project_map::DependencyId {
            from: module("map-builder"),
            to: module(used),
        };
        assert!(
            map.dependencies.contains_key(&dependency),
            "map-builder uses {used}"
        );
    }
    let project_map::Review::Judged { judgment, .. } =
        &map.modules[&module("map-builder")].description
    else {
        panic!("map-builder is judged")
    };
    assert_eq!(judgment.kind, project_map::ModuleKind::Logic);
}
