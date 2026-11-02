#!/usr/bin/env python3
"""Which workspace packages a pull request's tests must cover.

A PR runs the tests of the crates its change can reach, so most PRs stop
waiting on the whole workspace. Pushes to main and the release PR still run
everything, so every test keeps running on every change, just not always
before merge.

The rule is deliberately cautious:

- A file under a crate's directory changes that crate, and so every crate
  that depends on it by any kind of dependency: normal, dev or build. A test
  binary is built from its crate's dev-dependencies too.
- Prose that no test reads (`knowledge/`, `plans/`, Markdown at the root,
  and `docs/` apart from the registry) changes nothing.
- A `Cargo.lock` change reaches every package that depends, by any path in
  the lock file, on an entry that changed. The lock file lists dev and
  build dependencies too.
- A root `Cargo.toml` change that only adds or removes members, or changes
  `[workspace.dependencies]`, reaches the packages using a changed
  dependency; a new member is already reached through its own directory.
  Any other change to it, such as a profile or lint, runs every test.
- `docs/status/systems.toml` reaches `emu198x-fleet-web`, whose
  conformance test reads it.
- Anything else, such as `test-data/`, a workflow or a script, runs every
  test. Tests read those
  by path, and a path can't be traced through the dependency graph.

Prints JSON: {"mode": "all" | "some" | "none", "packages": [...], "why": ...}.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

REPO = Path(__file__).resolve().parents[2]

# Read by no test; checked by the docs and registry jobs instead.
PROSE_DIRS = ("knowledge/", "plans/", "docs/")
# Files outside any crate that named crates' tests read by path. The
# collector reads the registry too, but only to attribute results.
READ_BY_TESTS = {"docs/status/systems.toml": {"emu198x-fleet-web"}}
# The parts of the root manifest whose changes can be traced to packages.
TRACED = {"members", "dependencies"}


def changed_files(base: str, head: str) -> list[str]:
    proc = subprocess.run(
        ["git", "diff", "--name-only", f"{base}...{head}"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    )
    return [line for line in proc.stdout.splitlines() if line]


def file_at(ref: str, path: str) -> str:
    proc = subprocess.run(
        ["git", "show", f"{ref}:{path}"],
        cwd=REPO,
        capture_output=True,
        text=True,
    )
    return proc.stdout if proc.returncode == 0 else ""


def manifest_changes(before: str, after: str) -> set[str] | None:
    """Workspace dependencies whose entry changed, or None if more did.

    None means the root manifest changed in a way that can reach any crate,
    such as a profile, a lint or the workspace package version.
    """
    old = tomllib.loads(before) if before else {}
    new = tomllib.loads(after) if after else {}

    def untraced(manifest: dict) -> dict:
        workspace = manifest.get("workspace", {})
        return {
            **{k: v for k, v in manifest.items() if k != "workspace"},
            "workspace": {k: v for k, v in workspace.items() if k not in TRACED},
        }

    if untraced(old) != untraced(new):
        return None
    old_deps = old.get("workspace", {}).get("dependencies", {})
    new_deps = new.get("workspace", {}).get("dependencies", {})
    return {
        name
        for name in old_deps.keys() | new_deps.keys()
        if old_deps.get(name) != new_deps.get(name)
    }


def lock_changes(before: str, after: str) -> set[str]:
    """Packages a lock-file change can reach, by name.

    An entry changed if it was added, removed, or differs in version,
    source or dependencies. Everything that depends on a changed entry,
    in either lock file, is reached too.
    """
    old = tomllib.loads(before).get("package", []) if before else []
    new = tomllib.loads(after).get("package", []) if after else []

    def entries(packages: list[dict]) -> dict[str, list[dict]]:
        by_name: dict[str, list[dict]] = {}
        for package in packages:
            by_name.setdefault(package["name"], []).append(package)
        return {name: sorted(group, key=json.dumps) for name, group in by_name.items()}

    old_entries, new_entries = entries(old), entries(new)
    changed = {
        name
        for name in old_entries.keys() | new_entries.keys()
        if old_entries.get(name) != new_entries.get(name)
    }
    users: dict[str, set[str]] = {}
    for package in old + new:
        for dependency in package.get("dependencies", []):
            users.setdefault(dependency.split(" ")[0], set()).add(package["name"])
    reached: set[str] = set()
    stack = list(changed)
    while stack:
        name = stack.pop()
        if name in reached:
            continue
        reached.add(name)
        stack.extend(users.get(name, ()))
    return reached


def workspace() -> tuple[dict[str, str], dict[str, set[str]]]:
    """Package name -> directory, and package -> packages that depend on it."""
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    )
    meta = json.loads(proc.stdout)
    root = Path(meta["workspace_root"])
    dirs = {
        p["name"]: Path(p["manifest_path"]).parent.relative_to(root).as_posix()
        for p in meta["packages"]
    }
    # Every dependency, external ones included, so a changed
    # `[workspace.dependencies]` entry can be traced to the crates using it.
    dependents: dict[str, set[str]] = {name: set() for name in dirs}
    for package in meta["packages"]:
        for dependency in package["dependencies"]:
            dependents.setdefault(dependency["name"], set()).add(package["name"])
    return dirs, dependents


def affected(
    files: list[str],
    dirs: dict[str, str],
    dependents: dict[str, set[str]],
    lock: set[str] | None = None,
    manifest: set[str] | None = None,
) -> dict:
    """`lock` and `manifest` are what `lock_changes` and `manifest_changes`
    found, when those files changed; None runs every test."""
    direct: set[str] = set()
    for file in files:
        if file == "Cargo.lock" and lock is not None:
            direct |= lock & dirs.keys()
            continue
        if file == "Cargo.toml" and manifest is not None:
            direct |= manifest
            continue
        path = PurePosixPath(file)
        owner = max(
            (
                name
                for name, directory in dirs.items()
                if directory not in ("", ".")
                and path.is_relative_to(PurePosixPath(directory))
            ),
            key=lambda name: len(dirs[name]),
            default=None,
        )
        if owner:
            direct.add(owner)
        elif file in READ_BY_TESTS:
            direct |= READ_BY_TESTS[file]
        elif file.startswith(PROSE_DIRS) or (
            "/" not in file and file.endswith(".md")
        ):
            continue
        else:
            return {"mode": "all", "packages": [], "why": f"{file} is outside any crate"}
    reached = set()
    stack = list(direct)
    while stack:
        name = stack.pop()
        if name in reached:
            continue
        reached.add(name)
        stack.extend(dependents.get(name, ()))
    reached &= dirs.keys()
    if not reached:
        return {"mode": "none", "packages": [], "why": "no file a test can read changed"}
    return {
        "mode": "some",
        "packages": sorted(reached),
        "why": f"{len(direct)} changed crates reach {len(reached)}",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", help="the PR's base commit")
    parser.add_argument("head", nargs="?", default="HEAD")
    args = parser.parse_args()
    dirs, dependents = workspace()
    files = changed_files(args.base, args.head)
    lock = manifest = None
    base = subprocess.run(
        ["git", "merge-base", args.base, args.head],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    # Compared from the same starting point as the file diff, which `...`
    # takes from the merge base, so changes landed on main since don't count.
    if "Cargo.lock" in files:
        lock = lock_changes(file_at(base, "Cargo.lock"), file_at(args.head, "Cargo.lock"))
    if "Cargo.toml" in files:
        manifest = manifest_changes(
            file_at(base, "Cargo.toml"), file_at(args.head, "Cargo.toml")
        )
    result = affected(files, dirs, dependents, lock, manifest)
    print(json.dumps(result))
    print(f"{result['mode']}: {result['why']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
