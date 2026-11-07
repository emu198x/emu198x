#!/usr/bin/env python3
"""Reject workspace examples that write the same executable or Windows PDB."""

from __future__ import annotations

import argparse
import json
import subprocess
from collections import defaultdict


def check(metadata: dict) -> int:
    outputs: dict[str, list[str]] = defaultdict(list)
    for package in metadata["packages"]:
        for target in package["targets"]:
            if "example" in target["kind"]:
                # Windows output paths are case-insensitive. Package names do
                # not qualify target/debug/examples/<name>.exe or its PDB.
                outputs[target["name"].casefold()].append(package["name"])
    if not outputs:
        raise ValueError("No workspace examples discovered")
    collisions = [
        f"{name}: {', '.join(packages)}"
        for name, packages in sorted(outputs.items())
        if len(packages) > 1
    ]
    if collisions:
        raise ValueError("Example output filename collisions:\n" + "\n".join(collisions))
    return sum(map(len, outputs.values()))


def self_test() -> None:
    def package(name: str, target: str, kind: str = "example") -> dict:
        return {"name": name, "targets": [{"name": target, "kind": [kind]}]}

    for bad in (
        {"packages": []},
        {"packages": [package("first", "parity"), package("second", "parity")]},
        {"packages": [package("first", "Parity"), package("second", "parity")]},
    ):
        try:
            check(bad)
        except ValueError:
            pass
        else:
            raise AssertionError(f"Accepted invalid example outputs: {bad}")
    good = {
        "packages": [
            package("first", "first_parity"),
            package("second", "second_parity"),
            package("library", "first_parity", "lib"),
        ]
    }
    assert check(good) == 2


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    )
    count = check(json.loads(metadata.stdout))
    print(f"{count} workspace examples have distinct output filenames")


if __name__ == "__main__":
    main()
