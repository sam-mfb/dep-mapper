#!/usr/bin/env python3
"""Build a fresh compiler index for kkt and discover paths given only a supplier package."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory

PROOF = Path(__file__).resolve().parent
PIN = "4c520c70618845c90fd62c0e3c6a9961cbf3dbff"


def output(args, cwd, **kwargs):
    return subprocess.check_output(args, cwd=cwd, text=True, **kwargs)


def discover(checkout, package, build):
    host = next(line.removeprefix("host: ") for line in output(["rustc", "-vV"], checkout).splitlines()
                if line.startswith("host: "))
    if "-linux-" not in host or os.environ.get("CARGO_BUILD_TARGET", host) != host:
        raise ValueError("this experiment requires a native Linux build")
    targets = build / "target"
    dumps = build / "mir"
    env = dict(os.environ, RUSTC_BOOTSTRAP="1", CARGO_INCREMENTAL="0",
               CARGO_PROFILE_DEV_DEBUG="0", CARGO_TARGET_DIR=str(targets),
               PROOF_DUMPS=str(dumps), RUSTC_WORKSPACE_WRAPPER=str(PROOF.parent / "capture"))
    meta = json.loads(output(["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"], checkout))
    units = {}
    for p in meta["packages"]:
        for target in p["targets"]:
            if target["kind"] in (["lib"], ["bin"]):
                units.setdefault(target["name"].replace("-", "_"), []).append((p["name"], target))
    supplier = [name for name, entries in units.items()
                if any(pkg == package and t["kind"] == ["lib"] for pkg, t in entries)]
    if len(supplier) != 1:
        raise ValueError(f"expected one library target for {package}")
    subprocess.run(["cargo", "build", "--quiet", "--locked", "-p", "kashkeshet", "--bin", "kkt", "--lib"],
                   cwd=checkout, env=env, check=True)
    binary = build / "find"
    subprocess.run(["rustc", "--edition", "2024", "-Dwarnings", "-O", PROOF / "find.rs", "-o", binary],
                   cwd=checkout, check=True)
    query = [binary, dumps, supplier[0]]
    docs = []
    for owner in output(query + ["owners"], checkout).splitlines():
        matches = [(pkg, t) for pkg, t in units[owner] if (dumps / (owner + "-" + t["kind"][0])).is_dir()]
        if len(matches) != 1:
            raise ValueError(f"ambiguous compiled target {owner}")
        pkg, target = matches[0]
        select = ["--lib"] if target["kind"] == ["lib"] else ["--bin", target["name"]]
        subprocess.run(["cargo", "rustdoc", "--quiet", "--locked", "-p", pkg, *select, "--",
                        "-Zunstable-options", "--output-format=json", "--document-private-items"],
                       cwd=checkout, env=env, check=True)
        docs.append(targets / "doc" / (owner + ".json"))
    index = build / "impls.tsv"
    index.write_text(output([sys.executable, PROOF / "impls.py", *docs], checkout), encoding="utf-8")
    return output(query + ["paths", index], checkout)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    parser.add_argument("supplier")
    args = parser.parse_args()
    checkout = args.checkout.resolve()
    if output(["git", "rev-parse", "HEAD"], checkout).strip() != PIN or output(["git", "status", "--porcelain"], checkout):
        parser.error(f"expected a clean checkout of {PIN}")
    with TemporaryDirectory() as temp:
        sys.stdout.write(discover(checkout, args.supplier, Path(temp)))
