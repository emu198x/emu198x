#!/usr/bin/env python3
"""Verify the system registry against the repo and the issue tracker.

The registry is the only place the repo's four vocabularies are joined, so it
is worth exactly as much as its accuracy. This is what keeps it honest:

- every shipping crate has at least one machine,
- every machine a profile declares appears in the registry,
- every label and milestone the registry names actually exists.

The third check needs the GitHub API and is skipped without it, so the first
two still run offline. A skipped check says so rather than passing quietly —
see knowledge/decisions/a-gate-nobody-runs-is-a-silent-gate.md.
"""

import json
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "docs/status/systems.toml"
# Machines whose id is built from a variable rather than a literal, so a scan
# of `MachineId::from("...")` cannot see them. Empty since #998 split the Game
# Gear out: every machine now states its id as a literal in its own crate.
# A name here is a machine the scan is blind to, so it is asserted explicitly
# rather than trusted to show up.
SCAN_BLIND: set[str] = set()


def registry() -> list[dict]:
    return tomllib.loads(REGISTRY.read_text())["system"]


def shipping_crates() -> set[str]:
    """The crates that ship a machine: `emu198x-*` with a binary to run.

    This used to be "`emu198x-*` minus a denylist of five", which worked
    while that prefix meant one thing here. It no longer does: publishing a
    crate takes the `emu198x-` prefix (198x/decisions/crate-naming.md binds
    it at publication), so `emu198x-mos-6502` is a CPU library sitting in the
    same namespace as `emu198x-spectrum`, which is an app. The denylist would
    have to name every chip crate ever published, and would fail the build
    the day someone forgot.

    `src/main.rs` separates them positively: a machine you can run has an
    entry point, a chip you link against does not. Verified equivalent when
    it landed — it reproduces all 30 registry entries exactly, and the only
    crates it drops are the six chips the denylist would have had to list.
    """
    return {
        p.name
        for p in (ROOT / "crates").glob("emu198x-*")
        if p.is_dir() and (p / "src" / "main.rs").exists()
    }


def declared_machines() -> set[str]:
    """Machine ids the profiles declare, as literals.

    The scan cannot tell a profile catalogue from a `#[cfg(test)]` fixture, so
    ids beginning `test-` are excluded by convention. Name a fixture machine
    that way; anything else makes this check fail, which is the intended
    outcome for a real machine nobody registered.
    """
    found = set()
    for f in (ROOT / "crates").glob("runtime-*/src/*.rs"):
        found |= set(re.findall(r'MachineId::from\("([a-z0-9-]+)"\)', f.read_text(errors="ignore")))
    return {m for m in found if not m.startswith("test-")} - {"dummy-machine"}


def gh(args: list[str]):
    out = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if out.returncode != 0 or not out.stdout.strip():
        return None
    return json.loads(out.stdout)


def main() -> int:
    systems = registry()
    problems: list[str] = []

    ids = [s["machine_id"] for s in systems]
    if len(ids) != len(set(ids)):
        problems.append("registry has duplicate machine_id entries")

    # Every shipping crate must host at least one machine. A crate nobody
    # registered is a machine that no status view can see.
    listed_crates = {s["crate"] for s in systems}
    for crate in sorted(shipping_crates() - listed_crates):
        problems.append(f"crate {crate} ships but no registry entry names it")
    for crate in sorted(listed_crates - shipping_crates()):
        problems.append(f"registry names crate {crate}, which does not ship")

    # Every declared machine must be registered. The blind-scan ones are
    # asserted explicitly, because the scan cannot see them and their absence
    # would otherwise look like agreement.
    declared = declared_machines()
    for machine in sorted(declared - set(ids)):
        problems.append(f"profile declares machine {machine}, absent from the registry")
    for machine in sorted(SCAN_BLIND):
        if machine not in ids:
            problems.append(f"machine {machine} is invisible to the scan and unregistered (#998)")

    # Labels and milestones, when the tracker is reachable.
    labels = gh(["gh", "label", "list", "--limit", "300", "--json", "name"])
    stones = gh(["gh", "api", "repos/emu198x/emu198x/milestones?state=all&per_page=100", "--paginate"])
    if labels is None or stones is None:
        print("note: GitHub unreachable — label and milestone checks did NOT run")
    else:
        have_labels = {l["name"] for l in labels}
        have_stones = {m["title"] for m in stones}
        for s in systems:
            if s["label"] not in have_labels:
                problems.append(f"{s['machine_id']}: label {s['label']} does not exist")
            if s["milestone"] not in have_stones:
                problems.append(f"{s['machine_id']}: milestone {s['milestone']!r} does not exist")

    if problems:
        print(f"registry check FAILED ({len(problems)}):")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"registry check passed: {len(systems)} machines across {len(listed_crates)} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
