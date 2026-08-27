#!/usr/bin/env python3
"""Checks that `../`-relative Markdown references in the repo actually resolve.

Doc comments cite decision records and knowledge pages by relative path, and
those paths were written at four mutually incompatible depths: from the crate
root instead of the repo root, mostly off by exactly one `../`. Thirteen of
nineteen went nowhere. Nothing noticed, because rustdoc does not render
`knowledge/` and nothing else ever followed them.

The links are for a person reading the source, so the convention is
file-relative: resolve from the directory of the file that contains the link.

## Resolution is answered from git, never from the filesystem

The first version of this check asked the filesystem whether a target existed.
That made its answer a property of the machine it ran on: it passed here and
failed in CI with 88 unresolved references, none of which were broken. Two
whole classes of target exist on a developer's disk and in no CI checkout —
files this repo deliberately gitignores, and files in sibling repos entirely.

So every question here is put to git, which answers identically everywhere:

- **Tracked** — in `git ls-files`. Resolves.
- **Deliberately untracked** — matched by `.gitignore`. Most of `knowledge/`
  is a local working notebook; `knowledge/chips/`, `knowledge/systems/` and
  friends are ignored on purpose. A link into one is correct for the person
  reading the source, who has it. Resolves.
- **In the repo and neither** — nothing can produce this file. Fails, with the
  `../` depth that would have worked.
- **Outside the repo** — the sibling docs repo and the 198x umbrella, reached
  by climbing past the root. A single-repo checkout cannot see them, so they
  are counted and reported rather than checked. Claiming to verify these
  would only re-describe whoever ran the check's disk layout.

The one exception among those is a tree that exists nowhere at all, listed in
`RETIRED_TREES` — dead as text, so it can be caught as text.

## What is not checked

- `docs/plans/` and `docs/brainstorms/` — historical documents that the docs
  repo's current-state rule freezes. Rot there is expected, and churning it
  would rewrite the record of what was true when it was written.
- References inside backticks that are *examples* of the convention rather
  than links. `knowledge/SCHEMA.md` teaches "use relative links" by showing
  `[Z80](../chips/zilog-z80.md)` — correct from a page one level down, and
  wrong if "fixed" to resolve from SCHEMA.md itself.

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

# Trees that exist on no machine. `Emu198x-Reference/` was the old reference
# library; the family's prose sources now live in `198x/reference/`. A path
# naming one of these is dead wherever it is read, so it is caught by name
# rather than by looking, and the backlog below holds the known ones.
RETIRED_TREES = ("Emu198x-Reference/",)

# The dead references that survive, because the paths inside the retired tree
# do not map one-for-one onto the new library: repointing them is research,
# not arithmetic. This list may shrink, never grow — a new dead reference
# fails, and so does a line here that no longer matches, so it cannot rot.
BACKLOG = Path(__file__).parent / "doc-links-backlog.txt"


def load_backlog() -> set[str]:
    if not BACKLOG.exists():
        return set()
    return {
        line.strip()
        for line in BACKLOG.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    }


def git(*args: str, stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, cwd=ROOT, input=stdin
    )


def tracked_files() -> set[str]:
    listed = git("ls-files", "-z")
    listed.check_returncode()
    return {rel for rel in listed.stdout.split("\0") if rel}


def ignored_files(paths: set[str]) -> set[str]:
    """Which of `paths` `.gitignore` covers — asked in one call.

    `git check-ignore` answers for paths that do not exist, which is the whole
    point: CI must reach the same verdict as a machine that has the file.
    """
    if not paths:
        return set()
    listed = git("check-ignore", "-z", "--stdin", stdin="\0".join(sorted(paths)))
    # Exit 1 simply means nothing matched; only 128 and up are real errors.
    if listed.returncode > 1:
        raise RuntimeError(f"git check-ignore failed: {listed.stderr.strip()}")
    return {rel for rel in listed.stdout.split("\0") if rel}


def skipped(rel: str) -> bool:
    return any(rel == d or rel.startswith(f"{d}/") for d in SKIP_DIRS)


def inside_repo(path: Path, ref: str) -> str | None:
    """`ref` as a repo-relative path, or None if it climbs out of the repo."""
    target = (path.parent / ref.split("#")[0]).resolve()
    try:
        return str(target.relative_to(ROOT))
    except ValueError:
        return None


def references(text: str, comments_only: bool) -> list[tuple[int, str]]:
    """Yields (line number, reference) for every `../`-relative .md reference."""
    found = []
    for number, line in enumerate(text.split("\n"), 1):
        if comments_only and not line.strip().startswith("//"):
            continue
        for match in REF.finditer(line):
            found.append((number, match.group(1)))
    return found


def depths(path: Path, ref: str, resolvable: set[str]) -> list[int]:
    """The `../` depths that would make `ref` resolve from `path`."""
    tail = ref
    while tail.startswith("../"):
        tail = tail[3:]
    return [
        depth
        for depth in range(1, 10)
        if (rel := inside_repo(path, "../" * depth + tail)) is not None
        and rel in resolvable
    ]


def targets() -> list[tuple[Path, bool]]:
    """The files scanned: tracked Rust sources and tracked `knowledge/` pages."""
    listed = git("ls-files", "-z", "*.rs", "knowledge/*.md", "knowledge/**/*.md")
    listed.check_returncode()
    return [
        (ROOT / rel, rel.endswith(".rs"))
        for rel in listed.stdout.split("\0")
        if rel and not skipped(rel) and rel not in ILLUSTRATIVE
    ]


def classify(path: Path, ref: str, tracked: set[str], ignored: set[str]) -> str:
    """Where `ref` points, from `path`: the verdict git can give everywhere."""
    if any(tree in ref for tree in RETIRED_TREES):
        return "retired"
    rel = inside_repo(path, ref)
    if rel is None:
        return "outside"
    if rel in tracked:
        return "resolves"
    if rel in ignored:
        return "ignored"
    return "missing"


def self_test() -> int:
    """Samples lifted out of the tree, not written to fit the checker.

    Every one of these was a false failure or a silent pass at some point.
    The two verdicts that cost a CI run — a target this repo ignores on
    purpose, and a target in another repo — are here as the exact lines that
    produced them.
    """
    shape = [
        # The real shape: a Markdown link inside a `//!` comment.
        ("//! [x](../../knowledge/decisions/nes-clock-topology.md)", 1),
        # With an anchor — the fragment must not defeat the check.
        ("/// [x](../../knowledge/decisions/nes-clock-topology.md#pin-contracts)", 1),
        # Code, not a comment: a path in a string literal is not a doc link.
        ('let p = "../../knowledge/decisions/nes-clock-topology.md";', 0),
        # No `../` prefix at all — not the shape this checks.
        ("//! see knowledge/decisions/nes-clock-topology.md", 0),
    ]

    # (file, reference, expected verdict) — all four taken from the tree.
    verdicts = [
        # Tracked: `knowledge/processes/` is ignored except by name, and this
        # page is one of the named exceptions.
        (
            "knowledge/decisions/amiga-accuracy-closure-campaign.md",
            "../processes/amiga-test-kit-video-conformance.md",
            "resolves",
        ),
        # Ignored on purpose: `knowledge/systems/` is a local notebook. CI
        # called this broken; it is correct for whoever is reading the source.
        (
            "crates/common-nintendo-game-boy/src/timing.rs",
            "../../../knowledge/systems/nintendo-game-boy/timing.md",
            "ignored",
        ),
        # Another repo: climbs out of the root into the 198x umbrella.
        (
            "knowledge/decisions/continuity-and-succession.md",
            "../../../../decisions/emu198x-best-in-class.md",
            "outside",
        ),
        # A tree that exists nowhere, caught by name rather than by looking.
        (
            "knowledge/decisions/spectrum-test-oracle-priority.md",
            "../../../Emu198x-Reference/_organised/known-unknowns.md",
            "retired",
        ),
        # One `../` short of the ignored case above: still inside the repo,
        # and nothing can produce it. This is the failure the check exists
        # for, and the one the other four must not drown out.
        (
            "crates/common-nintendo-game-boy/src/timing.rs",
            "../../knowledge/systems/nintendo-game-boy/timing.md",
            "missing",
        ),
    ]

    failures = 0
    fake = ROOT / "crates" / "nonexistent" / "src" / "lib.rs"
    for source, expected in shape:
        got = len(references(source, comments_only=True))
        if got != expected:
            print(f"self-test FAILED: {source!r} — want {expected} refs, got {got}")
            failures += 1

    tracked = tracked_files()
    wanted = {
        rel
        for name, ref, _ in verdicts
        if (rel := inside_repo(ROOT / name, ref)) is not None
    }
    ignored = ignored_files(wanted - tracked)
    for name, ref, expected in verdicts:
        got = classify(ROOT / name, ref, tracked, ignored)
        if got != expected:
            print(f"self-test FAILED: {name} -> {ref} — want {expected}, got {got}")
            failures += 1

    if failures:
        return 1
    print(f"self-test: {len(shape) + len(verdicts)} cases pass")
    return 0


def main() -> int:
    if "--self-test" in sys.argv and self_test() != 0:
        return 1

    scanned = targets()
    found = [
        (path, number, ref)
        for path, comments_only in scanned
        for number, ref in references(path.read_text(errors="replace"), comments_only)
    ]

    tracked = tracked_files()
    # Every path any verdict could turn on, resolved in one `check-ignore`.
    wanted: set[str] = set()
    for path, _, ref in found:
        tail = ref
        while tail.startswith("../"):
            tail = tail[3:]
        for depth in range(0, 10):
            probe = ref if depth == 0 else "../" * depth + tail
            if (rel := inside_repo(path, probe)) is not None:
                wanted.add(rel)
    ignored = ignored_files(wanted - tracked)

    resolvable = tracked | ignored
    backlog = load_backlog()
    seen_dead: set[str] = set()
    problems: list[str] = []
    counts = {"resolves": 0, "ignored": 0, "outside": 0, "retired": 0, "missing": 0}

    for path, number, ref in found:
        rel = path.relative_to(ROOT)
        verdict = classify(path, ref, tracked, ignored)
        counts[verdict] += 1
        if verdict in ("resolves", "ignored", "outside"):
            continue
        if verdict == "retired":
            key = f"{rel}\t{ref}"
            seen_dead.add(key)
            if key in backlog:
                continue
            hint = (
                f"{next(t for t in RETIRED_TREES if t in ref)} no longer exists "
                "anywhere — repoint it at 198x/reference/, or add it to "
                f"{BACKLOG.name} with a reason"
            )
        else:
            options = depths(path, ref, resolvable)
            hint = (
                f"use {'../' * options[0]}{ref.lstrip('./')}"
                if len(options) == 1
                else f"no depth from 1 to 9 resolves it — is {ref} the right target?"
                if not options
                else f"ambiguous: depths {options} all resolve"
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
        f"doc references OK — {counts['resolves']} resolve, "
        f"{counts['ignored']} point at files this repo ignores on purpose, "
        f"{counts['outside']} point outside the repo and cannot be checked "
        f"from here, {counts['retired']} are dead and listed in "
        f"{BACKLOG.name}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
