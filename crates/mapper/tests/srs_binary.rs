//! The `srs` binary as a person runs it.

use std::process::Command;

fn srs(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_srs"))
        .args(args)
        .env_remove("ANTHROPIC_API_KEY")
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
fn map_without_an_api_key_stops_before_analysis() {
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
        "llm",
        "--out",
        "unused.toml",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "srs: --evaluator llm needs a Claude API key in ANTHROPIC_API_KEY\n"
    );
}

/// A fresh folder holding the example map saved as `base.toml` and `draft`
/// as `draft.toml`.
fn draft_folder(name: &str, draft: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("srs-draft-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base = map_persist::save(&map_persist::MapDocument::Map(
        project_map::example::sharsheret(),
    ));
    std::fs::write(dir.join("base.toml"), base).unwrap();
    std::fs::write(dir.join("draft.toml"), draft).unwrap();
    dir
}

const DRAFT: &str = "format = 1\nkind = \"draft\"\nbase = \"base.toml\"\n\n[modules.map-cache]\nname = \"map-cache\"\nreview = { state = \"unreviewed\" }\n";

#[test]
fn draft_check_passes_a_sound_draft_and_fails_on_problems() {
    let dir = draft_folder("check", DRAFT);
    let draft = dir.join("draft.toml");
    let output = srs(&["draft", "check", draft.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");

    std::fs::write(&draft, DRAFT.replace("unreviewed", "unknown")).unwrap();
    let output = srs(&["draft", "check", draft.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("{}:7:", draft.display())),
        "{stderr}"
    );
    assert!(stderr.contains("the draft has 1 problem(s)"), "{stderr}");
}

#[test]
fn draft_render_writes_the_page() {
    let dir = draft_folder("render", DRAFT);
    let out = dir.join("draft.html");
    let output = srs(&[
        "draft",
        "render",
        dir.join("draft.toml").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    assert!(std::fs::read_to_string(out).unwrap().contains("map-cache"));
}
