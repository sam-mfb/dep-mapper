//! The map of sharsheret checked in at docs/architecture/sharsheret-map.toml
//! is the project's end-to-end example. It must keep loading, fitting
//! together and showing as the crates change. The README in this crate gives
//! the command that refreshes it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn checked_in() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/sharsheret-map.toml")
}

#[test]
fn the_checked_in_map_loads_and_fits_together() {
    let text = std::fs::read_to_string(checked_in()).expect("the map is checked in");
    let map_persist::MapDocument::Map(map) = map_persist::load(&text).expect("the map loads")
    else {
        panic!("a saved map, not a draft")
    };
    assert_eq!(map.problems(), vec![]);
    for crate_name in [
        "mapper",
        "map-builder",
        "project-graph",
        "graph-evaluator",
        "project-map",
    ] {
        let module = &map.modules[&project_map::ModuleId(crate_name.into())];
        assert!(
            matches!(module.description, project_map::Review::Judged { .. }),
            "{crate_name} is judged"
        );
    }
    let mapper_uses_builder = project_map::DependencyId {
        from: project_map::ModuleId("mapper".into()),
        to: project_map::ModuleId("map-builder".into()),
    };
    assert!(map.dependencies.contains_key(&mapper_uses_builder));
}

#[test]
fn srs_show_renders_the_checked_in_map() {
    let page = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sharsheret-map.html");
    if page.exists() {
        std::fs::remove_file(&page).expect("old page removed");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_srs"))
        .arg("show")
        .arg(checked_in())
        .arg("--page")
        .arg(&page)
        .output()
        .expect("srs runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = std::fs::read_to_string(&page).expect("the page is written");
    assert!(written.contains("map-builder"));
}
