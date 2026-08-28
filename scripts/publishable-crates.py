#!/usr/bin/env python3
"""List publishable workspace crates in local dependency order."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def metadata() -> dict:
    completed = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout)


def publish_order(data: dict) -> list[dict[str, str]]:
    members = set(data["workspace_members"])
    packages = {
        package["name"]: package
        for package in data["packages"]
        if package["id"] in members and package["publish"] != []
    }
    pending = set(packages)
    ordered: list[dict[str, str]] = []
    while pending:
        ready = sorted(
            name
            for name in pending
            if not {
                dependency["name"]
                for dependency in packages[name]["dependencies"]
                if dependency["name"] in pending
            }
        )
        if not ready:
            raise RuntimeError(f"publishable dependency cycle: {sorted(pending)}")
        for name in ready:
            package = packages[name]
            ordered.append(
                {
                    "name": name,
                    "version": package["version"],
                    "manifest": str(pathlib.Path(package["manifest_path"]).relative_to(ROOT)),
                }
            )
            pending.remove(name)
    return ordered


def main() -> int:
    crates = publish_order(metadata())
    if not crates:
        print("no publishable workspace crates", file=sys.stderr)
        return 1
    json.dump(crates, sys.stdout, separators=(",", ":"))
    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
