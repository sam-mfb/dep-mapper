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
        assert len(witnesses) == 1
        provider = witnesses[0]["provider"]
        assert provider["item"]["name"].endswith("::ClaudeCommand")
        assert any(s["owner"]["crate"] == "kkt_agent_commands" for s in provider["steps"])
        assert any(s["target"]["name"] == "kkt_llm_agent_command::Claude::" + method
                   for s in provider["implementation"]["calls"])
    summary = json.loads((report / "summary.json").read_text())
    routes = json.loads((report / "routes.json").read_text())
    assert summary["calls"]["dynamic"] == len(routes) == summary["withWitness"] + summary["withoutWitness"]
    assert all(route["providers"] or route["stops"] for route in routes)
    print("PASS kkt direct calls, full injection, two adapted methods, and accounted virtual sites")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
