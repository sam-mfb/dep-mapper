"""Classify every selected call by what evidence actually connects it to a provider."""
import argparse
from collections import Counter
import json
from pathlib import Path


def load(path):
    return json.loads(path.read_text())


def source_key(row):
    return (row["owner"]["crate"], row["owner"]["name"], row["at"], row["block"],
            row["member"]["crate"], row["member"]["name"])


def classify(directory, prior=None):
    routes = [r for r in load(directory / "routes.json") if r.get("bound")]
    structure = {source_key(r): r["paths"] for r in load(directory / "structural.json")}
    keys = {source_key(r) for r in routes}
    if keys != structure.keys() or len(keys) != len(routes):
        raise ValueError("structural and contextual inventories disagree")
    baseline = None
    if prior:
        baseline = {(r["callerCrate"], r["caller"], r["span"], r["block"], r["contractCrate"],
                     r["method"]): bool(r["after"]) for r in load(prior)["ledger"]}
        if baseline.keys() != keys:
            raise ValueError("baseline describes a different set of call sites")
    counts, transitions, declarations = Counter(), Counter(), Counter()
    rows = []
    for r in sorted(routes, key=source_key):
        k = source_key(r)
        b = r["bound"]
        paths = structure[k]
        evidence = ("contextual" if r["providers"] else
                    "structural candidate only" if paths else "no recovered path")
        counts[evidence] += 1
        types = [d["nominal"] for d in b["declared"]]
        # T, references, tuples and other non-ADT receivers do not imply a
        # single nominal supplier even if the impl declaration is unique.
        crates = sorted({t["crate"] for t in types if t})
        units = {t["unit"] for t in types if t}
        unique = bool(types) and all(types) and len(units) == 1
        declarations["one nominal type crate" if unique else "open or multiple type crates"] += 1
        row = dict(caller=k[1], crate=k[0], source=k[2], block=k[3], member=k[5],
                   contractCrate=k[4], declaredTypeCrates=crates, singleTypeCrate=unique,
                   implPatterns=[d["selfPattern"] for d in b["declared"]],
                   declarationsWithoutMethod=sum(d.get("explicitMethod") is None for d in b["declared"]),
                   compiledTypes=[t["name"] for t in b["compiled"]], evidence=evidence,
                   contextualTypes=sorted({p["item"]["name"] for p in r["providers"]}),
                   structuralTypes=sorted({p["provider"]["name"] for p in paths}),
                   stops=sorted({s["reason"] for s in r["stops"]}))
        if baseline is not None:
            row["previouslyWitnessed"] = baseline[k]
            transitions[("previously witnessed" if baseline[k] else "previously missing") + " / " + evidence] += 1
        rows.append(row)
    return dict(build=load(directory / "build.json"), sites=len(rows), paths=counts,
                declarations=declarations, comparison=transitions, ledger=rows)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--prior-ledger", type=Path)
    args = parser.parse_args()
    result = classify(args.report, args.prior_ledger)
    (args.report / "limits.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "ledger"}, indent=2))
