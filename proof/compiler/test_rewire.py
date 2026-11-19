"""Counterfactuals on a disposable copy of the measured kkt revision."""
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

HERE = Path(__file__).resolve().parent
PIN = "41fbc289dbfe68be90840d107e217762229b9745"


def test(checkout, tmp):
    src = tmp / "source"
    subprocess.run(["git", "clone", "--quiet", "--shared", checkout, src], check=True)
    subprocess.run(["git", "checkout", "--quiet", "--detach", PIN], cwd=src, check=True)
    native = src / "src/machine/native.rs"
    code = native.read_text()
    assert code.count("output: &NativeOutput,") > 1
    native.write_text(code.replace("output: &NativeOutput,", "output: &SilentOutput,", 1) + """
struct SilentOutput;
impl kkt_install_commands::Output for SilentOutput {
    fn output(&self, _: Arguments<'_>) {}
    fn errors(&self, _: Arguments<'_>) {}
}
""")
    admission = src / "crates/kkt-admission/src/lib.rs"
    code = admission.read_text()
    assert code.count("Admission::Bearer => &AnyBearer,") == 1
    admission.write_text(code.replace("Admission::Bearer => &AnyBearer,", "Admission::Bearer => &Open,"))
    report = tmp / "report"
    subprocess.run([HERE / "run", src, report, "-p", "kashkeshet", "--bin", "kkt", "--lib"], check=True)
    routes = json.loads((report / "routes.json").read_text())
    status = [r for r in routes if r["owner"]["name"]=="lifecycle::status::{closure#0}" and
              r["member"]["name"]=="Output::output"]
    assert status and all({p["item"]["name"] for p in r["providers"]}=={"machine::native::SilentOutput"} for r in status)
    assert any(p["item"]["name"]=="machine::native::NativeOutput" for r in routes for p in r["providers"])
    decide, = (r for r in routes if r["owner"]["name"]=="Admission::decide")
    choices = {(variant,p["item"]["name"]) for p in decide["providers"] for g in p["enumChoices"]
               if g["enumName"]=="Admission" for variant in g["variants"]}
    assert choices == {("Open","open::Open"),("Bearer","open::Open"),("SharedKey","shared_key::SharedKey")}
    print(f"PASS all {len(status)} async status output sites change provider; other NativeOutput uses survive")
    print("PASS the Bearer arm now delivers Open; SharedKey keeps its own provider")


if __name__ == "__main__":
    with TemporaryDirectory() as root:
        test(Path(sys.argv[1]).resolve(),Path(root))
