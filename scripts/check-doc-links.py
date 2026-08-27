#!/usr/bin/env python3
"""Checks that `../`-relative Markdown references in the repo actually resolve.

Doc comments cite decision records and knowledge pages by relative path, and
those paths were written at four mutually incompatible depths: from the crate
root instead of the repo root, mostly off by exactly one `../`. Thirteen of
nineteen went nowhere. Nothing noticed, because rustdoc does not render
`knowledge/` and nothing else ever followed them.

The links are for a person reading the source, so the convention is
file-relative: resolve from the directory of the file that contains the link.

## What is checked, and what is not

Checked: every `../`-prefixed `.md` reference in a Rust comment, and in the
Markdown under `knowledge/`.

Not checked:

- `docs/plans/` and `docs/brainstorms/` — historical documents that the docs
  repo's current-state rule freezes. Rot there is expected, and churning it
  would rewrite the record of what was true when it was written.
- References inside backticks that are *examples* of the convention rather
  than links. `knowledge/SCHEMA.md` teaches "use relative links" by showing
  `[Z80](../chips/zilog-z80.md)` — correct from a page one level down, and
  wrong if "fixed" to resolve from SCHEMA.md itself.

Cross-tree links are allowed and do resolve: `knowledge/` reaches the sibling
docs repo and the 198x umbrella with enough `../`, which is an established
pattern here rather than an accident.

Run `--self-test` to check the detector itself.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# A `../`-prefixed path ending in .md, however it is delimited — these appear
# as Markdown link targets, and bare in backticks.
REF = re.compile(r"((?:\.\./)+[A-Za-z0-9_./-]+\.md)")

SKIP_DIRS = ("target", "docs/plans", "docs/brainstorms")

# Lines that show a link rather than make one. `SCHEMA.md` documents the
# cross-reference convention by example; its paths are correct for the pages
# it describes and wrong from where they sit.
ILLUSTRATIVE = {"knowledge/SCHEMA.md"}

# References whose target tree no longer exists, so no depth can resolve them
# and none can be invented. Chiefly `Emu198x-Reference/`, the old reference
# library — the family's prose sources now live in `198x/reference/`, but the
# paths inside the old tree do not map across one-for-one, so repointing them
# is research rather than arithmetic. Listed rather than guessed at.
#
# This list may shrink, never grow: a new dead reference fails, and so does a
# line here that no longer matches, so it cannot rot.
BACKLOG = Path(__file__).parent / "doc-links-backlog.txt"


def load_backlog() -> set[str]:
    if not BACKLOG.exists():
        return set()
    return {
        line.strip()
        for line in BACKLOG.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    }


def skipped(rel: str) -> bool:
    return any(rel == d or rel.startswith(f"{d}/") for d in SKIP_DIRS)


def scan_text(text: str, path: Path, comments_only: bool) -> list[tuple[int, str]]:
    """Yields (line number, reference) for every unresolved reference."""
    broken = []
    for number, line in enumerate(text.split("\n"), 1):
        if comments_only and not line.strip().startswith("//"):
            continue
        for match in REF.finditer(line):
            ref = match.group(1)
            if not (path.parent / ref.split("#")[0]).resolve().exists():
                broken.append((number, ref))
    return broken


def candidates(path: Path, ref: str) -> list[int]:
    """The `../` depths that would make `ref` resolve from `path`."""
    tail = ref
    while tail.startswith("../"):
        tail = tail[3:]
    return [
        depth
        for depth in range(1, 10)
        if (path.parent / ("../" * depth + tail.split("#")[0])).resolve().exists()
    ]


def targets() -> list[tuple[Path, bool]]:
    """Tracked files only, via `git ls-files`.

    Most of `knowledge/` is gitignored — only `decisions/` and a handful of
    named process files are tracked, the rest being a local working notebook.
    Walking the filesystem would scan files CI never sees, so the check would
    pass here and fail there, and the backlog below would list entries that do
    not exist in a fresh clone.
    """
    listed = subprocess.run(
        ["git", "ls-files", "-z", "*.rs", "knowledge/*.md", "knowledge/**/*.md"],
        capture_output=True,
        text=True,
        check=True,
        cwd=ROOT,
    ).stdout.split("\0")

    found: list[tuple[Path, bool]] = []
    for rel in listed:
        if not rel or skipped(rel) or rel in ILLUSTRATIVE:
            continue
        found.append((ROOT / rel, rel.endswith(".rs")))
    return found


def self_test() -> int:
    """Samples taken from the tree, not written to fit the regex."""
    cases = [
        # The real shape: a Markdown link inside a `//!` comment.
        ("//! [nes-clock-topology.md](../../knowledge/decisions/nes-clock-topology.md)", 1),
        # With an anchor — the fragment must not defeat the existence check.
        ("/// [x](../../knowledge/decisions/nes-clock-topology.md#pin-contracts)", 1),
        # Code, not a comment: a path in a string literal is not a doc link.
        ('let p = "../../knowledge/decisions/nes-clock-topology.md";', 0),
        # No `../` prefix at all — not the shape this checks.
        ("//! see knowledge/decisions/nes-clock-topology.md", 0),
    ]
    failures = 0
    fake = ROOT / "crates" / "nonexistent" / "src" / "lib.rs"
    for source, expected in cases:
        # .rs files are always scanned comments-only; that is the point of
        # the code sample below.
        got = len(scan_text(source, fake, comments_only=True))
        if got != expected:
            print(f"self-test FAILED: {source!r} — want {expected} broken, got {got}")
            failures += 1
    if failures:
        return 1
    print(f"self-test: {len(cases)} cases pass")
    return 0


def main() -> int:
    if "--self-test" in sys.argv and self_test() != 0:
        return 1

    backlog = load_backlog()
    seen_dead: set[str] = set()
    problems: list[str] = []
    for path, comments_only in targets():
        text = path.read_text(errors="replace")
        if "../" not in text:
            continue
        for number, ref in scan_text(text, path, comments_only):
            rel = path.relative_to(ROOT)
            depths = candidates(path, ref)
            if not depths:
                key = f"{rel}\t{ref}"
                seen_dead.add(key)
                if key in backlog:
                    continue
            hint = (
                f"use {'../' * depths[0]}{ref.lstrip('./').lstrip('/')}"
                if len(depths) == 1
                else f"no depth from 1 to 9 resolves it — is {ref} the right target?"
                if not depths
                else f"ambiguous: depths {depths} all resolve"
            )
            problems.append(f"{rel}:{number}\n    {ref}\n    {hint}")

    for stale in sorted(backlog - seen_dead):
        problems.append(
            f"{stale.replace(chr(9), ' -> ')}\n"
            f"    listed in {BACKLOG.name} but no longer a dead reference — "
            "delete the line"
        )

    if problems:
        print(f"{len(problems)} unresolved doc reference(s):\n")
        for problem in problems:
            print(f"  {problem}\n")
        return 1

    print(
        f"doc references OK — every ../-relative .md reference resolves "
        f"({len(seen_dead)} dead ones in the backlog, none new)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
