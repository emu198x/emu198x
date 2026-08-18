#!/usr/bin/env python3
"""Collect per-machine test evidence for the status canon.

Status drifted (#825) because every claim about a machine was a claim:
a milestone someone closed, a `support_tier` someone typed, a sentence in
a README. None of it was checked, so all of it aged. This script produces
the other half — what the test suite actually *did* — so a claim can be
put next to its evidence and the gap between them read off.

## What counts as evidence

A test that ran and passed. Nothing else. In particular:

- An `#[ignore]`d test is **absence of evidence**, not a pass.
- A test that hit `emu198x_test_skip::skip!` for a missing fixture is
  absence of evidence, even though libtest printed `ok`. That silent
  `ok` is what let the Dragon golden-frame test sit broken in CI for
  three months (see `emu198x-test-skip`).
- A closed milestone and a declared `support_tier` are claims. They are
  reported by the comparison step, never by this one.

## How a test is attributed to a machine

Through the dependency graph, not through names. A machine's shipping
crate (`emu198x-c64`) transitively depends on a set of workspace crates;
that closure is the code the machine is built from. A crate in exactly
one shipping closure is **exclusive** to that machine, and its tests are
that machine's own evidence. A crate in several closures is **shared**
(`cpu-z80`, `emu198x-shell`), and its tests are evidence for all of them
but distinguish none of them.

The distinction is the point. A machine whose only passing tests live in
`cpu-z80` has no machine-specific evidence at all, however green CI looks.

## What this does not do

It does not derive a support tier. Counts cannot: a hundred passing unit
tests on a memory map do not establish that a machine boots, and one
golden-frame test may establish more than all of them. Deriving a tier
needs tests to declare what they verify, which they do not yet. This
script reports the ledger; it does not grade it.

Doctests are not collected — rustdoc runs them, not a test binary, so
they are outside the executable-level attribution used here.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
REGISTRY = REPO / "docs" / "status" / "systems.toml"

# libtest's per-test result line. Stable across every edition this
# workspace has used. `ignored` may carry a reason after a comma.
RESULT_LINE = re.compile(r"^test (?P<name>\S+) \.\.\. (?P<outcome>.+)$")


def run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=REPO, text=True, capture_output=True, **kwargs)


def workspace_metadata() -> tuple[dict[str, set[str]], dict[str, str], dict[str, str]]:
    """Crate -> transitive workspace dependencies, and package id -> name.

    Normal dependencies only. A dev-dependency is a crate's test scaffolding,
    not part of the machine, and following those edges would make every
    machine depend on every test helper.

    The id map exists because `--message-format json` identifies a test
    binary's package by id, and ids are not parseable by eye: the name
    appears after the `#` only when it differs from the directory.
    """
    meta = json.loads(
        run(["cargo", "metadata", "--format-version", "1", "--no-deps"]).stdout
    )
    ids = {p["id"]: p["name"] for p in meta["packages"]}
    names = {p["name"] for p in meta["packages"]}
    roots = {p["name"]: str(Path(p["manifest_path"]).parent) for p in meta["packages"]}
    direct = {
        p["name"]: [
            d["name"] for d in p["dependencies"] if d["name"] in names and d["kind"] is None
        ]
        for p in meta["packages"]
    }

    def closure(root: str) -> set[str]:
        seen: set[str] = set()
        stack = [root]
        while stack:
            crate = stack.pop()
            if crate in seen:
                continue
            seen.add(crate)
            stack.extend(direct.get(crate, ()))
        return seen

    return {name: closure(name) for name in names}, ids, roots


def test_executables(ids: dict[str, str]) -> list[tuple[str, str, Path]]:
    """Every test binary cargo would run, as (package, target, path).

    `--no-run` builds them and reports each one's package, which is the
    only reliable way to attribute an integration test: the file name of
    `tests/paging.rs` is `paging-<hash>` and says nothing about which
    crate it belongs to.
    """
    proc = run(
        [
            "cargo", "test", "--workspace", "--no-run",
            "--message-format", "json",
        ]
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        raise SystemExit("cargo test --no-run failed; evidence would be partial")

    found = []
    for line in proc.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        if not msg.get("executable") or not msg.get("profile", {}).get("test"):
            continue
        package = ids.get(msg["package_id"])
        if package is None:
            # Only workspace members are collected; a proc-macro or build
            # dependency with tests of its own is not this repo's evidence.
            continue
        found.append((package, msg["target"]["name"], Path(msg["executable"])))
    return found


def run_one(package: str, target: str, exe: Path, root: str, skip_dir: Path) -> dict:
    """Run one test binary and return its outcome tally.

    Each binary gets its own skip log, so a skip is attributed to the
    crate that raised it. A single shared log could not be: the line
    records the test's thread name, which carries no crate.

    Running the binary rather than letting `cargo test` drive it is what
    makes attribution exact, but it means reproducing what cargo sets up:
    the working directory is the package root, not the workspace root,
    and `CARGO_MANIFEST_DIR` is readable at runtime. Tests locate their
    fixtures through both, and getting either wrong would turn a passing
    test into a fabricated failure.
    """
    log = skip_dir / f"{package}--{target}.tsv"
    env = dict(os.environ)
    env["EMU198X_SKIP_LOG"] = str(log)
    env["CARGO_MANIFEST_DIR"] = root
    env["CARGO_PKG_NAME"] = package
    # Never strict here: this collects what happened, it does not judge
    # whether the fixtures should have been present.
    env.pop("EMU198X_STRICT_FIXTURES", None)

    proc = subprocess.run(
        [str(exe)], cwd=root, env=env, text=True, capture_output=True
    )

    passed, failed, ignored = [], [], []
    for line in proc.stdout.splitlines():
        match = RESULT_LINE.match(line)
        if not match:
            continue
        name, outcome = match["name"], match["outcome"]
        if outcome == "ok":
            passed.append(name)
        elif outcome.startswith("ignored"):
            ignored.append(name)
        elif outcome.startswith("FAILED"):
            failed.append(name)

    skipped = []
    if log.exists():
        for line in log.read_text().splitlines():
            test, _, reason = line.partition("\t")
            skipped.append({"test": test, "reason": reason})

    # A skip is recorded by a test that libtest counted as passing.
    # Leaving it in both columns would let absence of evidence show up
    # as evidence, which is the exact failure this exists to prevent.
    # The skip log records the test's thread name, which libtest sets to
    # the test's full path — the same string it prints in the result line.
    skipped_names = {entry["test"] for entry in skipped}
    passed = [name for name in passed if name not in skipped_names]

    return {
        "package": package,
        "target": target,
        "passed": sorted(passed),
        "failed": sorted(failed),
        "ignored": sorted(ignored),
        "skipped": sorted(skipped, key=lambda s: s["test"]),
        "crashed": proc.returncode not in (0, 101),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out", type=Path, default=REPO / "target" / "status-evidence.json"
    )
    parser.add_argument(
        "--jobs", type=int, default=min(8, os.cpu_count() or 4),
        help="test binaries to run at once",
    )
    args = parser.parse_args()

    registry = tomllib.loads(REGISTRY.read_text())["system"]
    closures, ids, roots = workspace_metadata()

    shipping = sorted({system["crate"] for system in registry})
    unknown = [crate for crate in shipping if crate not in closures]
    if unknown:
        raise SystemExit(f"registry names crates not in the workspace: {unknown}")

    # A crate reachable from exactly one shipping crate is that machine's
    # own; anything else is shared and cannot distinguish machines.
    reach: dict[str, set[str]] = {}
    for crate in shipping:
        for member in closures[crate]:
            reach.setdefault(member, set()).add(crate)

    binaries = test_executables(ids)
    print(f"running {len(binaries)} test binaries", file=sys.stderr)

    with tempfile.TemporaryDirectory(prefix="emu198x-evidence-") as tmp:
        skip_dir = Path(tmp)
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            results = list(
                pool.map(
                    lambda b: run_one(b[0], b[1], b[2], roots[b[0]], skip_dir),
                    binaries,
                )
            )

    by_package: dict[str, dict] = {}
    for result in results:
        tally = by_package.setdefault(
            result["package"],
            {"passed": 0, "failed": 0, "ignored": 0, "skipped": 0, "targets": []},
        )
        for key in ("passed", "failed", "ignored", "skipped"):
            tally[key] += len(result[key])
        tally["targets"].append(result)

    systems = []
    for system in sorted(registry, key=lambda s: s["machine_id"]):
        crate = system["crate"]
        own = sorted(c for c in closures[crate] if reach[c] == {crate})
        shared = sorted(c for c in closures[crate] if len(reach[c]) > 1)
        # Two machines shipping from one crate cannot have separate own
        # evidence — #998 is the request to split exactly that case.
        cohabiting = sorted(
            s["machine_id"] for s in registry
            if s["crate"] == crate and s["machine_id"] != system["machine_id"]
        )
        systems.append(
            {
                "machine_id": system["machine_id"],
                "crate": crate,
                "shares_crate_with": cohabiting,
                "own_crates": own,
                "shared_crates": shared,
                "own": roll_up(own, by_package),
                "shared": roll_up(shared, by_package),
            }
        )

    orphans = sorted(set(by_package) - set(reach))
    payload = {
        "systems": systems,
        "packages": {
            name: {k: v for k, v in tally.items() if k != "targets"}
            for name, tally in sorted(by_package.items())
        },
        "unattributed_packages": orphans,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(f"wrote {args.out}", file=sys.stderr)
    return 0


def roll_up(crates: list[str], by_package: dict[str, dict]) -> dict:
    total = {"passed": 0, "failed": 0, "ignored": 0, "skipped": 0}
    for crate in crates:
        tally = by_package.get(crate)
        if not tally:
            continue
        for key in total:
            total[key] += tally[key]
    return total


if __name__ == "__main__":
    raise SystemExit(main())
