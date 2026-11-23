"""Integration experiment. Only assertions and fixture edits know the expected route."""
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

from run import PIN, discover, output


def query(src, supplier, work, name):
    build = work / name
    build.mkdir()
    return discover(src, supplier, build)


def client_paths(report):
    return [path for path in report.split("PATH ")[1:]
            if "invoked by kkt_llm_agent_config::files::apply_claude" in path]


def assert_client(report, adapter):
    paths = client_paths(report)
    assert len(paths) == 2, paths
    for method in ("add_server", "remove_server"):
        candidates = [p for p in paths if p.startswith("kkt_llm_agent_command::Claude::" + method + " -> ")]
        assert len(candidates) == 1
        path = candidates[0]
        assert f"::{adapter} implements kkt_llm_agent_config::command::ClientCommand::{method} @" in path
        assert "store client_command @" in path and "pass to kkt_agent_commands::connect::run @" in path


def test(checkout, work):
    src = work / "subject"
    subprocess.run(["git", "clone", "--quiet", "--shared", checkout, src], check=True)
    subprocess.run(["git", "checkout", "--quiet", "--detach", PIN], cwd=src, check=True)
    supplier = "kkt-llm-agent-command"
    original = query(src, supplier, work, "baseline")
    assert_client(original, "ClaudeCommand")
    baseline = work / "baseline"
    replay = output([baseline / "find", baseline / "mir", supplier, "paths", baseline / "impls.tsv"], src)
    assert original == replay
    print("PASS original supplier-only query and byte-identical replay", flush=True)

    terminal = query(src, "kkt-terminal", work, "terminal")
    assert any(p.startswith("kkt_terminal::Terminal::output -> ") and "::TerminalOutput implements" in p
               and "invoked by kkt_agent_commands::connect::run @" in p for p in terminal.split("PATH ")[1:])
    print("PASS independent supplier query", flush=True)

    for file in (src / "src").rglob("*.rs"):
        source = file.read_text()
        if "ClaudeCommand" in source:
            file.write_text(source.replace("ClaudeCommand", "RenamedBridge"))
    renamed = query(src, supplier, work, "renamed")
    assert_client(renamed, "RenamedBridge")
    assert "ClaudeCommand" not in renamed
    print("PASS discovered renamed adapter", flush=True)

    # The old adapter still compiles, calls the provider, and is supplied elsewhere.
    # Change only the object supplied on the connect path.
    config = src / "src/machine/client_config.rs"
    with config.open("a") as f:
        f.write("""
pub struct OtherCommand;
impl ClientCommand for OtherCommand {
    fn remove_server(&self, _: Option<&Path>) -> Result<(), CommandError> { Ok(()) }
    fn add_server(&self, _: &str, _: Option<&Path>) -> Result<(), CommandError> { Ok(()) }
}
""")
    parts = src / "src/machine/agent_parts.rs"
    before = parts.read_text()
    after = before.replace("RenamedBridge", "OtherCommand")
    assert after != before
    parts.write_text(after)
    cut = query(src, supplier, work, "rewired")
    assert not client_paths(cut), cut
    print("PASS route disappears when a different implementation is supplied", flush=True)


if __name__ == "__main__":
    with TemporaryDirectory() as root:
        test(Path(sys.argv[1]).resolve(), Path(root))
