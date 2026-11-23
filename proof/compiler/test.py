"""Independent oracles for the compiler inventory, provider provenance and its boundaries."""
import json
import os
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory

HERE = Path(__file__).resolve().parent


def collect(source, directory, *flags, external=False):
    args = [HERE / "run", *(["--include-dependencies"] if external else []), source, directory, *flags]
    subprocess.run(args, check=True)
    return json.loads((directory / "routes.json").read_text())


def query(report, supplier):
    data = subprocess.check_output(["node", HERE / "query.ts", report, supplier], text=True)
    return [json.loads(line) for line in data.splitlines()]


def main(temp):
    src = temp / "sample"
    shutil.copytree(HERE / "sample", src)
    report = temp / "baseline"
    routes = collect(src, report)
    by_name = {route["owner"]["name"]: route for route in routes}
    assert len(routes) == len(by_name) == 12
    providers = lambda route: {p["item"]["name"] for p in route["providers"]}
    expected = {"Bridge", "sensor::Factory"}
    assert providers(by_name["display"]) == expected
    assert not providers(by_name["contradictory"])
    assert any("opposite requirements" in e["reason"] for e in by_name["contradictory"]["excluded"])
    assert providers(by_name["from_generic"]) == {"Bridge"}
    assert providers(by_name["gated"]) == {"sensor::Factory"}
    assert providers(by_name["inverted"]) == {"sensor::Factory"}
    assert providers(by_name["independent"]) == expected
    assert providers(by_name["deferred::{closure#0}"]) == {"sensor::Factory"}
    assert providers(by_name["shared"]) == {"Bridge"}
    assert providers(by_name["active_only"]) == {"Bridge"}
    assert providers(by_name["build_choice"]) == {"Spare"}
    assert providers(by_name["redirected"]) == {"Spare"}
    independent = by_name["independent"]["providers"]
    for hit in independent:
        observed = {(c["name"],c["value"]) for c in hit["conditions"]}
        assert observed == {("right",True),("left",hit["item"]["name"]=="Bridge")}
    assert any(c["variants"]==["Active"] for c in by_name["active_only"]["providers"][0]["enumChoices"])
    assert not by_name["after_edit"]["providers"]
    assert any(s["reason"] == "mutable borrow on incoming path" for s in by_name["after_edit"]["stops"])
    bridge = next(p for p in by_name["display"]["providers"] if p["item"]["name"] == "Bridge")
    assert {"forward", "main"} <= {step["owner"]["name"] for step in bridge["steps"]}
    assert [h["item"]["name"] for h in bridge["implementation"]["externalReturnSources"]["hits"]] == ["sensor::measured"]
    assert any(c["target"]["name"] == "sensor::audit" for c in bridge["implementation"]["calls"])
    facts = query(report, "sensor")
    categories = {f["kind"] for f in facts}
    assert {"implements","public-use","path","supply","call","injected-implementation","adapter-returns-supplier-result"} <= categories
    summary = json.loads((report / "summary.json").read_text())
    assert summary["calls"]["indirect"] == 1
    assert summary["calls"]["trait"] >= 1
    assert summary["units"] == summary["unitEdges"] == 3
    env = dict(os.environ, CARGO_TARGET_DIR=str(temp / "executable"))
    subprocess.run(["cargo","+1.98.1","build","--locked","-p","console"],cwd=src,env=env,check=True)
    exe = temp / "executable/debug/console"
    for args, expected_stdout in [([], "11"), (["adapted"], "22")]:
        assert subprocess.check_output([exe, *args], text=True).strip() == expected_stdout
    print("PASS supplier-only query, full and adapted injection, control-flow counterexample, mutable origin and runtime oracle",flush=True)

    baseline_routes = (report / "routes.json").read_bytes()
    subprocess.run(["node", HERE / "report.ts", report],check=True)
    assert baseline_routes == (report / "routes.json").read_bytes()
    rebuilt = temp / "rebuilt"
    collect(src, rebuilt)
    # Cargo may give separate wrapper paths different crate hashes. Compare the
    # semantic evidence, including source spans and all route steps, across builds.
    def stable(value):
        if isinstance(value,list):
            return sorted((stable(v) for v in value),key=lambda v:json.dumps(v,sort_keys=True))
        if isinstance(value,dict):
            return {k:stable(v) for k,v in value.items() if k not in ("id","unit")}
        return value
    assert stable(routes) == stable(json.loads((rebuilt / "routes.json").read_text()))
    print("PASS identical report replay and independent rebuild",flush=True)

    manifest = src / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace('"sensor", ', '').replace('[workspace]', '[workspace]\nexclude = ["sensor"]'))
    external = temp / "external"
    feature_routes = collect(src, external, "--features", "console/calibration", external=True)
    assert providers(next(r for r in feature_routes if r["owner"]["name"]=="build_choice")) == {"Bridge"}
    external_facts = query(external,"sensor")
    def calibration(rows):
        return any(f["kind"]=="call" and f["evidence"]["owner"]["name"]=="calibrate" for f in rows)
    assert calibration(external_facts) and not calibration(facts)
    assert "injected-implementation" in {f["kind"] for f in external_facts}
    print("PASS feature selection and supplier outside the workspace",flush=True)


if __name__ == "__main__":
    with TemporaryDirectory() as temp:
        main(Path(temp))
