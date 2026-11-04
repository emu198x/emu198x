#!/usr/bin/env python3
"""Run every selected library's doctests; binary-only packages have none."""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any, TextIO

REPO = Path(__file__).resolve().parents[2]
LIBRARY_KINDS = {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}


def doctest_packages(metadata: dict[str, Any], selected: list[str]) -> list[str]:
    members = set(metadata["workspace_members"])
    packages = {p["id"]: p for p in metadata["packages"] if p["id"] in members}
    if not members or members != packages.keys():
        raise ValueError("incomplete workspace metadata; refusing partial doctest selection")
    by_name = {p["name"]: p for p in packages.values()}
    requested = set(selected) if selected else set(by_name)
    missing = requested - by_name.keys()
    if missing:
        raise ValueError(f"selected packages absent from workspace metadata: {sorted(missing)}")
    return sorted(
        name for name in requested
        if any(t["doctest"] and LIBRARY_KINDS.intersection(t["kind"])
               for t in by_name[name]["targets"])
    )


def run_doctests(selected: list[str], cwd: Path = REPO, *, output: TextIO | None = None) -> int:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=cwd, capture_output=True, text=True, check=True,
    )
    packages = doctest_packages(json.loads(result.stdout), selected)
    if not packages:
        print(f"No library doctest targets in selected packages: {', '.join(selected) or '(workspace)'}")
        return 0
    args = [arg for package in packages for arg in ["--package", package]]
    return subprocess.run(
        ["cargo", "test", "--doc", *args], cwd=cwd,
        stdout=output, stderr=subprocess.STDOUT if output is not None else None,
        check=False,
    ).returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-p", "--package", action="append", default=[])
    args = parser.parse_args()
    try:
        return run_doctests(args.package)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Doctest selection failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
