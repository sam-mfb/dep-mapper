"""Check supplier-only queries against the two source routes on the pinned kkt build."""
import json
from pathlib import Path
import sys

from test import query


def main(report):
    backup = query(report, "kkt-backup-os")
    serve = [r["evidence"] for r in backup if r["kind"] == "call" and
            r["evidence"]["owner"]["name"].startswith("commands::serve::")]
    assert {r["target"]["name"] for r in serve} >= {"kkt_backup::Backups::write", "kkt_backup::Backups::prune"}
    assert any(r["kind"] == "injected-implementation" and
               r["evidence"]["consumer"]["name"] == "lifecycle::local_backup" and
               r["evidence"]["consumer"]["crate"] == "kkt_install_commands" for r in backup)
    assert any(r["kind"] == "implements" and r["evidence"]["contract"]["crate"] == "kkt_backup" for r in backup)
    command = query(report, "kkt-llm-agent-command")
    routes = [r["evidence"] for r in command if r["kind"] == "injected-method-calls-supplier" and
              r["evidence"]["consumer"]["crate"] == "kkt_llm_agent_config"]
    for method in ("add_server", "remove_server"):
        witnesses = [r for r in routes if r["member"]["name"].endswith("::" + method)]
        paths = [w["provider"] for w in witnesses]
        assert paths and all(p["item"]["name"].endswith("::ClaudeCommand") for p in paths)
        branches = [{s["owner"]["crate"] for s in p["steps"]} for p in paths]
        assert any("kkt_agent_commands" in crates for crates in branches)
        if json.loads((report / "build.json").read_text())["scope"] == "dispatch":
            assert any("kkt_install_commands" in crates for crates in branches)
        assert all(any(s["target"]["name"] == "kkt_llm_agent_command::Claude::" + method
                       for s in p["implementation"]["calls"]) for p in paths)
    summary = json.loads((report / "summary.json").read_text())
    routes = json.loads((report / "routes.json").read_text())
    assert summary["calls"]["dynamic"] == len(routes) == summary["withWitness"] + summary["withoutWitness"]
    assert all(route["providers"] or route["stops"] or route["excluded"] for route in routes)
    decide, = (r for r in routes if r["owner"]["name"]=="Admission::decide")
    bindings = {(v,p["item"]["name"]) for p in decide["providers"] for c in p["enumChoices"]
                if c["enumName"]=="Admission" for v in c["variants"]}
    assert bindings == {("Open","open::Open"),("SharedKey","shared_key::SharedKey"),("Bearer","any_bearer::AnyBearer")}
    os_choices = [p["item"]["name"] for r in routes for p in r["providers"]
                  if p["owner"]["name"]=="machine::service::for_this_os"]
    assert set(os_choices)=={"kkt_bare_service_systemd::Systemd"}
    assert any(r["owner"]["name"]=="RotatingFile::open" and
               any(p["item"]["crate"]=="kkt_clock_system" for p in r["providers"]) for r in routes)
    print("PASS the log clock, runtime admission modes and Linux service selection")
    print("PASS kkt direct calls, full injection, two adapted methods, and accounted virtual sites")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
