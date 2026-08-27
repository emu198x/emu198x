#!/usr/bin/env python3
"""Compute and apply the next suite version.

This repo is one suite, one version, one changelog: all 215 crates carry
`version.workspace = true`, so the whole bump is a single line in the root
manifest. What is *not* free is the intra-workspace dependency requirements —
634 path dependencies request `version = "0.2.0"`, a caret range. A patch or
minor bump inside 0.2.x satisfies it; crossing to 0.3.0 does not, and cargo
refuses to resolve:

    error: failed to select a version for the requirement `commodore-agnus-ocs = "^0.2.0"`
    candidate versions found which didn't match: 0.3.0

So the requirements are rewritten to the new version's compatible base, which
is a no-op for the common case and only fires when the range actually moves.

Usage:
    prepare-release.py --print      # compute the next version, change nothing
    prepare-release.py --apply      # write the manifests
"""

import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def next_version() -> str:
    """The next version, from conventional commits since the last tag.

    Delegated to git-cliff so one tool owns the commit-to-semver rules that
    also produce the changelog. Bump behaviour is pinned in cliff.toml's
    [bump]: pre-1.0, a breaking marker moves the minor rather than declaring
    1.0.0, which is git-cliff's default and is not a decision one `fix!:`
    commit should make.
    """
    out = subprocess.run(
        ["git-cliff", "--bumped-version"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.strip().splitlines()[-1]
    return out.lstrip("v")


def compatible_base(version: str) -> str:
    """The lowest version sharing `version`'s caret range.

    Cargo's caret rules differ either side of 1.0: pre-1.0 the minor is the
    breaking axis (^0.2.0 admits 0.2.9, not 0.3.0), after it the major is.
    """
    major, minor, _ = version.split(".")
    return f"{major}.0.0" if major != "0" else f"0.{minor}.0"


def set_workspace_version(version: str) -> str | None:
    manifest = ROOT / "Cargo.toml"
    text = manifest.read_text()
    pattern = re.compile(r'(\[workspace\.package\](?:[^\[]*?)\nversion = ")([^"]+)(")', re.S)
    match = pattern.search(text)
    if not match:
        sys.exit("no [workspace.package] version found in Cargo.toml")
    previous = match.group(2)
    manifest.write_text(pattern.sub(rf'\g<1>{version}\g<3>', text, count=1))
    return previous


def independently_versioned() -> set[str]:
    """Crates that carry their own version instead of the suite's.

    The published chip crates dropped `version.workspace = true` so their
    version can tell a consumer when *they* changed rather than when the suite
    did. They are still path dependencies, so they look exactly like every
    other intra-workspace requirement — but their version does not move with
    the suite, and rewriting their requirement to a version they do not have
    is a resolution failure.
    """
    own = set()
    for manifest in sorted(ROOT.glob("crates/*/Cargo.toml")):
        if re.search(r'^version = "', manifest.read_text(), re.M):
            own.add(manifest.parent.name)
    return own


def rewrite_workspace_requirements(base: str) -> int:
    """Point every suite-versioned path dependency at `base`.

    Only path dependencies are touched: a `version` beside a `path` names a
    crate in this workspace, and rewriting an external crate's requirement
    would be a different and much worse bug.

    Crates with their own version are skipped — see `independently_versioned`.
    """
    own = independently_versioned()
    pattern = re.compile(r'(path = "\.\./([^"]+)", version = ")([^"]+)(")')

    def replace(match: re.Match[str]) -> str:
        if match.group(2) in own:
            return match.group(0)
        return f"{match.group(1)}{base}{match.group(4)}"

    changed = 0
    for manifest in sorted(ROOT.glob("crates/*/Cargo.toml")):
        text = manifest.read_text()
        new_text = pattern.sub(replace, text)
        if new_text != text:
            manifest.write_text(new_text)
            changed += sum(
                1 for m in pattern.finditer(text)
                if m.group(2) not in own and m.group(3) != base
            )
    return changed


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true", help="write the manifests")
    parser.add_argument("--print", dest="show", action="store_true", help="compute only")
    args = parser.parse_args()

    version = next_version()
    base = compatible_base(version)

    if args.show or not args.apply:
        print(version)
        return

    previous = set_workspace_version(version)
    changed = rewrite_workspace_requirements(base)
    print(f"version: {previous} -> {version}")
    print(f"intra-workspace requirements pointed at {base}: {changed} rewritten")


if __name__ == "__main__":
    main()
