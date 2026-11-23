"""Reconcile virtual calls to indexed contracts across two builds of the same source."""
import argparse
from collections import Counter
import json
from pathlib import Path


def read(directory, name):
    return json.loads((directory / name).read_text())


def sites(directory):
    units = {c["owner"]["unit"] for c in read(directory, "crates.json")}
    result = {}
    for site in read(directory, "routes.json"):
        if site["member"]["unit"] not in units:
            continue
        key = (site["owner"]["crate"], site["owner"]["name"], site["at"],
               site["block"], site["member"]["crate"], site["member"]["name"])
        if key in result:
            raise ValueError(f"ambiguous site {key}")
        result[key] = site
    return result


def provider_names(site):
    return sorted({(p["item"]["crate"], p["item"]["name"]) for p in site["providers"]})


def reasons(site):
    return sorted({s["reason"] for s in site["stops"]})


def compare(before, after):
    a, b = sites(before), sites(after)
    if a.keys() != b.keys():
        raise ValueError("the call-site inventories differ")
    counts = Counter()
    gaps = Counter()
    ledger = []
    for key in sorted(a):
        old, new = a[key], b[key]
        previous, current = provider_names(old), provider_names(new)
        state = ("retained" if current else "lost") if previous else ("recovered" if current else "unresolved")
        counts[state] += 1
        if not current:
            gaps[tuple(reasons(new))] += 1
        ledger.append(dict(callerCrate=key[0], caller=key[1], span=key[2], block=key[3],
                           contractCrate=key[4], method=key[5], status=state,
                           before=previous, after=current, priorStops=old["stops"],
                           currentStops=new["stops"], excluded=new.get("excluded", [])))
    return dict(sites=len(ledger), outcomes=counts, unresolvedReasons=[
        dict(reasons=list(k), sites=n) for k, n in gaps.most_common()], ledger=ledger)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--ledger", type=Path)
    args = parser.parse_args()
    report = compare(args.before, args.after)
    if args.ledger:
        args.ledger.write_text(json.dumps(report, indent=2))
    print(json.dumps({k:v for k,v in report.items() if k != "ledger"}, indent=2))
