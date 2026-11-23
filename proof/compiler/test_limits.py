"""Check downstream openness and compare structural candidates with all 256 program inputs."""
import json
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory

HERE = Path(__file__).resolve().parent


def package(root, name, dependencies, source, binary=False):
    directory = root / name
    (directory / "src").mkdir(parents=True)
    deps = "".join(f'{n} = {{ path = "../{n}" }}\n' for n in dependencies)
    (directory / "Cargo.toml").write_text(
        f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\n' + deps)
    (directory / "src" / ("main.rs" if binary else "lib.rs")).write_text(source)


def run(root):
    workspace = root / "fixture"
    workspace.mkdir()
    names = ["contract", "left", "right", "first", "second", "closed", "blanket", "generic_app"]
    (workspace / "Cargo.toml").write_text('[workspace]\nresolver = "3"\nmembers = ' + json.dumps(names))
    package(workspace, "contract", [], """
pub trait Port { fn value(&self) -> u8; }
pub fn consume(port: &dyn Port) -> u8 { port.value() }
""")
    for name, consumer, value in [("left", "first", 1), ("right", "second", 2)]:
        package(workspace, name, ["contract"], f"""
#[derive(Clone, Copy)]
pub struct Provider;
impl contract::Port for Provider {{ fn value(&self) -> u8 {{ {value} }} }}
""")
        package(workspace, consumer, ["contract", name],
                f"fn main() {{ println!(\"{{}}\", contract::consume(&{name}::Provider)); }}", True)
    package(workspace, "closed", ["contract", "left", "right"], """
fn result(input: u8) -> u8 {
    let port: &dyn contract::Port =
        if input & 1 == 1 { &right::Provider } else { &left::Provider };
    std::hint::black_box(port);
    if input % 2 == 0 { contract::consume(port) } else { 0 }
}
fn main() {
    for input in u8::MIN..=u8::MAX { println!("{}", result(input)); }
}
""", True)
    package(workspace, "blanket", [], """
pub trait Tag { fn tag(&self) -> u8; }
impl<T: Copy> Tag for T { fn tag(&self) -> u8 { 7 } }
pub fn probe(value: &dyn Tag) -> u8 { value.tag() }
""")
    package(workspace, "generic_app", ["blanket", "left", "right"], """
fn main() {
    println!("{}", blanket::probe(&left::Provider) + blanket::probe(&right::Provider));
}
""", True)

    def cargo(*cmd):
        return subprocess.check_output(["cargo", "+1.98.1", *cmd], cwd=workspace, text=True,
                                       env=dict(os.environ, CARGO_TARGET_DIR=str(root / "runtime")))
    cargo("generate-lockfile")
    for name, expected in [("first", {"left"}), ("second", {"right"}), ("closed", {"left", "right"})]:
        directory = root / name
        subprocess.run([HERE / "run", "--dispatch-bounds", workspace, directory, "-p", name,
                        "--bin", name], check=True)
        site, = json.loads((directory / "limits.json").read_text())["ledger"]
        assert site["caller"] == "consume"
        assert set(site["declaredTypeCrates"]) == expected
        assert site["singleTypeCrate"] == (len(expected) == 1)
        paths, = json.loads((directory / "structural.json").read_text())
        assert {p["provider"]["crate"] for p in paths["paths"]} == expected
        observed = list(map(int, cargo("run", "-q", "-p", name).splitlines()))
        if name == "closed":
            assert observed == [1 if n % 2 == 0 else 0 for n in range(256)]
            assert len(site["compiledTypes"]) == 2
        else:
            assert observed == [1 if name == "first" else 2]
    generic = root / "generic"
    subprocess.run([HERE / "run", "--dispatch-bounds", workspace, generic, "-p", "generic_app",
                    "--bin", "generic_app"], check=True)
    row, = (r for r in json.loads((generic / "limits.json").read_text())["ledger"]
            if r["crate"] == "blanket" and r["caller"] == "probe")
    assert len(row["implPatterns"]) == 1 and not row["singleTypeCrate"]
    assert len(row["compiledTypes"]) == 2 and row["declaredTypeCrates"] == []
    assert cargo("run", "-q", "-p", "generic_app").strip() == "14"
    print("PASS a single blanket implementation with two concrete receiver crates")

    standalone = root / "standalone"
    subprocess.run([HERE / "run", workspace, standalone, "-p", "contract", "--lib"], check=True)
    site, = json.loads((standalone / "routes.json").read_text())
    assert not site.get("bound") and not site["providers"] and site["stops"]
    print("PASS one unchanged library, two downstream suppliers, both compiled candidates, "
          "and an exhaustive runtime oracle ruling out one candidate at the consumer")


if __name__ == "__main__":
    with TemporaryDirectory() as tmp:
        run(Path(tmp))
