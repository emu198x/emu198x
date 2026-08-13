#!/usr/bin/env python3
"""Keep release-plz's `changelog_include` in step with the workspace.

Emu198x releases as one suite at one version, and the whole suite changelog is
aggregated onto the `emu198x-spectrum` package via release-plz's
`changelog_include`. release-plz has no wildcard for that field — the
documentation is explicit that packages must be listed individually — so the
list is a hand-maintained mirror of the workspace, and a hand-maintained mirror
drifts.

It had drifted badly. The list named only the 28 per-system binary shells, so
the other 176 crates in the workspace — every `machine-*`, every chip, every
`format-*` and `runtime-*` — could never appear in a release note. Nine `fix:`
commits across nine machines merged on 2026-08-13, release-plz re-ran twice
afterwards, and the changelog it produced still said nothing but "read .szx
snapshots".

Nothing caught that, for exactly the reason nine Z80 machines ran at half speed
for months: no gate compared the configuration against the thing it was supposed
to mirror. This script is that gate.

Usage:

    scripts/check-release-changelog-coverage.py           # check, non-zero on drift
    scripts/check-release-changelog-coverage.py --fix     # rewrite the list

The workspace `members` list in `Cargo.toml` is the source of truth, so this
needs no network and no `cargo metadata` call.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE_MANIFEST = ROOT / "Cargo.toml"
RELEASE_PLZ = ROOT / "release-plz.toml"

# The package the suite changelog is written to. It cannot include itself.
AGGREGATOR = "emu198x-spectrum"

# Crates that are infrastructure rather than emulation: they ship no
# user-visible machine behaviour, so their commits do not belong in release
# notes. Everything else in the workspace does — including the chip, format and
# runtime crates, which is where most real behaviour changes actually land.
INFRASTRUCTURE = {
    "emu198x-catalogue",
    "emu198x-native-video",
    "emu198x-shell",
    "emu198x-test-skip",
    "emu198x-ui",
}


def workspace_members() -> set[str]:
    """Every crate in the workspace, by package name.

    Member paths are `crates/<name>`, and every crate in this workspace is
    named for its directory, so the path is enough — no manifest parsing per
    crate.
    """
    manifest = tomllib.loads(WORKSPACE_MANIFEST.read_text())
    members = manifest.get("workspace", {}).get("members", [])
    names = set()
    for member in members:
        name = member.rstrip("/").rsplit("/", maxsplit=1)[-1]
        if name:
            names.add(name)
    return names


def expected_include() -> list[str]:
    """The list `changelog_include` should hold, sorted for a stable diff."""
    return sorted(workspace_members() - INFRASTRUCTURE - {AGGREGATOR})


def configured_include() -> list[str]:
    config = tomllib.loads(RELEASE_PLZ.read_text())
    for package in config.get("package", []):
        if package.get("name") == AGGREGATOR:
            return list(package.get("changelog_include", []))
    raise SystemExit(
        f"{RELEASE_PLZ.name} has no [[package]] entry for {AGGREGATOR}; "
        "the suite changelog has no aggregator."
    )


def render(names: list[str]) -> str:
    body = "".join(f'    "{name}",\n' for name in names)
    return f"changelog_include = [\n{body}]"


def rewrite(names: list[str]) -> None:
    text = RELEASE_PLZ.read_text()
    pattern = re.compile(r"changelog_include = \[.*?\n\]", re.DOTALL)
    if not pattern.search(text):
        raise SystemExit("could not find a changelog_include array to rewrite")
    RELEASE_PLZ.write_text(pattern.sub(lambda _: render(names), text, count=1))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fix",
        action="store_true",
        help="rewrite changelog_include to match the workspace",
    )
    args = parser.parse_args()

    expected = expected_include()
    configured = configured_include()

    missing = sorted(set(expected) - set(configured))
    extra = sorted(set(configured) - set(expected))

    if not missing and not extra:
        print(f"changelog_include covers all {len(expected)} releasable crates")
        return 0

    if args.fix:
        rewrite(expected)
        print(f"rewrote changelog_include: {len(expected)} crates")
        if missing:
            print(f"  added {len(missing)}")
        if extra:
            print(f"  removed {len(extra)}")
        return 0

    print("release-plz changelog_include is out of step with the workspace.\n")
    if missing:
        print(f"{len(missing)} crate(s) whose commits cannot reach the changelog:")
        for name in missing[:20]:
            print(f"  + {name}")
        if len(missing) > 20:
            print(f"  … and {len(missing) - 20} more")
    if extra:
        print(f"\n{len(extra)} listed crate(s) that no longer exist:")
        for name in extra:
            print(f"  - {name}")
    print("\nRun scripts/check-release-changelog-coverage.py --fix")
    return 1


if __name__ == "__main__":
    sys.exit(main())
