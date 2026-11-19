"""Index compiler-resolved trait implementations from the candidate importers' rustdoc JSON."""
import json
import sys
from pathlib import Path

for file in sorted(map(Path, sys.argv[1:])):
    doc = json.loads(file.read_text())
    if doc["format_version"] != 60:
        raise ValueError("unsupported rustdoc schema")
    index, paths = doc["index"], doc["paths"]
    for record in index.values():
        impl = record["inner"].get("impl", {})
        trait = impl.get("trait")
        concrete = impl.get("for", {}).get("resolved_path")
        span = record.get("span")
        if not trait or not concrete or not span or record["crate_id"] != 0:
            continue
        owner = paths.get(str(concrete["id"]))
        contract = paths.get(str(trait["id"]))
        if not owner or not contract:
            continue
        for item in impl["items"]:
            member = index[str(item)]
            if "function" in member["inner"]:
                print(file.stem, span["filename"], *span["begin"],
                      "::".join(owner["path"]), "::".join(contract["path"]), member["name"], sep="\t")
