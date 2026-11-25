//! The `srs` binary as a person runs it.

use std::process::Command;

fn srs(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_srs"))
        .args(args)
        .output()
        .expect("srs runs")
}

#[test]
fn help_lists_the_map_command() {
    let output = srs(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("map"));
}

#[test]
fn a_usage_mistake_exits_with_status_2() {
    let output = srs(&["map"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn map_says_which_piece_is_missing() {
    let output = srs(&[
        "map",
        ".",
        "--language",
        "rust",
        "--target",
        "lib",
        "--configuration",
        "default",
        "--evaluator",
        "manual",
        "--out",
        "unused.toml",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "srs: project-graph-rust is not built yet\n"
    );
}
