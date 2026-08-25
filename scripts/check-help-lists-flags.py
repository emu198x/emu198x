#!/usr/bin/env python3
"""Fail if a machine binary accepts a flag its `--help` never mentions.

`--mcp`, `--script` and `--headless` were wired into every machine and
absent from every `--help`, so the automation surface — arguably the most
interesting thing the emulator does — was undiscoverable from the tool
itself (#1175). The website tells readers a machine's flags come from
`--help`, which was true for the flags it listed and silently untrue for
these.

The Spectrum went the other way: it accepted `--script` but not
`--frames` or `--screenshot`, so the one machine the curriculum leads
with was the only one that needed a JSON file to take a picture (#1187).
Two independent sweeps then reported the wrong count, because reading
help text and running the binary disagreed.

Neither gap was someone forgetting once. Nothing compared the thirty
binaries, so this does.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Flags every machine binary dispatches on, and must therefore document.
AUTOMATION = ("--script", "--headless", "--mcp")

# Flags whose presence in the argument parser obliges a `--help` mention.
# Deliberately not every flag: some are machine-specific and documented in
# their own sections. These are the ones a reader is told to expect.
REQUIRED_IN_HELP = AUTOMATION

USAGE_RE = re.compile(r'const USAGE: &str = "\\\n(.*?)\n";', re.S)


def machine_crates(root: Path) -> list[Path]:
    return sorted(
        p for p in (root / "crates").glob("emu198x-*") if (p / "src" / "main.rs").is_file()
    )


def accepted_flags(crate: Path) -> set[str]:
    """Flags the crate's sources match on."""
    found: set[str] = set()
    for source in (crate / "src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8", errors="replace")
        found.update(re.findall(r'"(--[a-z0-9-]+)"', text))
    return found


def help_text(crate: Path) -> str:
    """Every USAGE block the crate defines, concatenated."""
    blocks: list[str] = []
    for source in (crate / "src").rglob("*.rs"):
        text = source.read_text(encoding="utf-8", errors="replace")
        blocks.extend(USAGE_RE.findall(text))
    return "\n".join(blocks)


def check(root: Path) -> list[str]:
    problems: list[str] = []
    for crate in machine_crates(root):
        accepted = accepted_flags(crate)
        documented = help_text(crate)
        if not documented:
            continue
        for flag in REQUIRED_IN_HELP:
            if flag in accepted and flag not in documented:
                problems.append(f"{crate.name}: accepts {flag} but --help never mentions it")
    return problems


def self_test() -> None:
    """The checker must fail on the shape it exists to catch.

    Without this, a detector that had stopped detecting would pass in
    silence — the same failure the fixture-guard check guards against.
    """
    good = 'const USAGE: &str = "\\\nUsage: x\n\nAutomation:\n    --script PATH   run\n";'
    assert USAGE_RE.findall(good), "the USAGE pattern must match the house format"
    assert "--script" in USAGE_RE.findall(good)[0]

    bad = 'const USAGE: &str = "\\\nUsage: x\n\nOptions:\n    --rom PATH  a rom\n";'
    assert "--script" not in USAGE_RE.findall(bad)[0], "a help block without the flag must not match"
    print("self-test: the pattern matches the house format and misses an undocumented flag")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args()

    if args.self_test:
        self_test()

    problems = check(args.root)
    if problems:
        print("Flags accepted but undocumented:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "\nA reader holding a release archive has no docs directory and no\n"
            "checkout; --help is the only per-machine reference that ships with\n"
            "the binary.",
            file=sys.stderr,
        )
        return 1
    print(f"{len(machine_crates(args.root))} machine binaries: every accepted automation flag is in --help")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
