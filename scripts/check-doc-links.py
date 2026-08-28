#!/usr/bin/env python3
"""Checks that the Markdown references in this repository resolve.

Doc comments and Markdown pages cite decision records, each other, and the
repository's own trees by relative path. Nothing follows those links
automatically — rustdoc does not render `knowledge/`, and no build step reads
`README.md` — so a reference that stopped resolving stays invisible until
someone tries to read one. This is what notices.

The links are for a person reading the source, so the convention is
file-relative: resolve from the directory of the file that contains the link.

There are two halves, and they fail in different ways.

**`../`-relative references** climb out of the file's own directory, so what
goes wrong is the depth: one `../` short and the path still lands inside the
repository, on nothing. Failures name the depth that would have worked.

**Repo-relative link targets** — `[x](docs/testing-policy.md)` — have no depth
to get wrong. What goes wrong is that the target leaves: a file moves to
another repository, or a directory is retired, and the link stays behind
pointing at nothing. These are read only from Markdown pages, and only where
they are actual link targets, because in Rust the convention is `../`-relative
and a repo-relative path in a doc comment is prose. A directory target
resolves when the tree holds anything, since a directory is not itself a
tracked object.

## Resolution is answered from git, never from the filesystem

Asking the filesystem whether a target exists makes the verdict a property of
the machine running the check. Two classes of target exist on a developer's
disk and in no CI checkout: files this repo deliberately gitignores, and files
in sibling repos. Git answers identically in both places, and answers about
paths that are not present, so every question here goes to git:

- **Tracked** — in `git ls-files`. Resolves.
- **Deliberately untracked** — matched by `.gitignore`. Most of `knowledge/`
  is a local working notebook; `knowledge/chips/`, `knowledge/systems/` and
  friends are ignored on purpose, and a link into one is correct for the
  person reading the source, who has it. Resolves.
- **In the repo and neither** — nothing can produce this file. Fails, naming
  the `../` depth that would have worked.
- **Outside the repo** — the sibling docs repo and the 198x umbrella, reached
  by climbing past the root. A single-repo checkout cannot see them, so they
  are counted and reported rather than checked: a verdict on them would
  describe the runner's disk layout rather than the reference.

`RETIRED_TREES` is the exception. A tree that exists nowhere at all is dead as
text, so it is caught as text.

## What is not checked

- `docs/plans/` and `docs/brainstorms/` — historical documents that the docs
  repo's current-state rule freezes. Rot there is expected, and churning it
  would rewrite the record of what was true when it was written.
- References inside backticks that are *examples* of the convention rather
  than links. `knowledge/SCHEMA.md` teaches "use relative links" by showing
  `[Z80](../chips/zilog-z80.md)` — correct from a page one level down, and
  wrong if "fixed" to resolve from SCHEMA.md itself.
- Repo-relative paths written in prose rather than linked. `RULES.md` cites
  globs and placeholder paths that way — `knowledge/decisions/*.md`,
  `docs/systems/<manufacturer>/<system>.md` — and neither names a real file.
- Anything a link target names other than a path here: a URL, any other
  scheme, a protocol-relative host, a bare anchor, an absolute path.

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

# A Markdown link target: the `x` in `](x)`. `REF` above asks whether a `../`
# depth is right; this asks whether a repo-relative target is still there at
# all, which is what a file moving to another repository breaks. Only Markdown
# pages are read for these: in Rust the convention is file-relative `../`, so a
# bare repo-relative path in a doc comment is prose, not a link.
LINK = re.compile(r"\]\(([^)\s]+)\)")

# Link targets that do not name a path in this repository: a URL or any other
# scheme, a protocol-relative host, a bare anchor, an absolute path.
NOT_LOCAL = re.compile(r"^(?:[A-Za-z][A-Za-z0-9+.-]*:|//|#|/)")

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


def local_links(text: str) -> list[tuple[int, str]]:
    """Yields (line number, target) for every repo-relative Markdown link."""
    found = []
    for number, line in enumerate(text.split("\n"), 1):
        for match in LINK.finditer(line):
            ref = match.group(1).split("#")[0].strip()
            # `../` targets are REF's half of the surface; an empty target is
            # a pure anchor that has already been stripped.
            if not ref or ref.startswith("../") or NOT_LOCAL.match(ref):
                continue
            found.append((number, ref))
    return found


def classify_local(
    path: Path, ref: str, tracked: set[str], ignored: set[str], dirs: set[str]
) -> str:
    """Where a repo-relative `ref` points, from `path`."""
    rel = inside_repo(path, ref)
    if rel is None:
        return "outside"
    if rel in tracked:
        return "resolves"
    # A link to a directory — `[crates/](crates)` — resolves when the tree has
    # anything in it. Directories are not themselves tracked objects.
    if rel in dirs:
        return "resolves"
    # `.gitignore` directory patterns carry a trailing slash — `knowledge/chips/`
    # — and git matches those only when it can tell the path is a directory. It
    # cannot, for a path that does not exist, which is every ignored directory
    # in a clean checkout. Asking in both forms keeps the verdict a property of
    # git rather than of the machine running the check.
    if rel in ignored or f"{rel}/" in ignored:
        return "ignored"
    return "missing"


def tracked_dirs(tracked: set[str]) -> set[str]:
    """Every directory that holds a tracked file, at any depth."""
    found: set[str] = set()
    for rel in tracked:
        parent = Path(rel).parent
        while str(parent) != ".":
            found.add(str(parent))
            parent = parent.parent
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
    """The files scanned: tracked Rust sources and tracked Markdown pages."""
    listed = git("ls-files", "-z", "*.rs", "*.md")
    listed.check_returncode()
    return [
        (ROOT / rel, rel.endswith(".rs"))
        for rel in listed.stdout.split("\0")
        if rel
        and not skipped(rel)
        and rel not in ILLUSTRATIVE
        # `CLAUDE.md` is a symlink to `AGENTS.md`; the same bytes read twice
        # report every problem twice.
        and not (ROOT / rel).is_symlink()
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
    """Samples taken from the tree, not written to fit the checker.

    A sample written from the author's picture of the target can only confirm
    that picture. These cover all five verdicts, and include the wrong-depth
    failure the check exists for, so the four passing verdicts cannot hide it.
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
        # Ignored on purpose: `knowledge/systems/` is a local notebook, so
        # this target is correct for whoever is reading the source.
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

    # The repo-relative half. A link that names a path in this repository is
    # right or wrong on its face, with no depth to get right.
    local_shape = [
        # A link to a directory, which is not itself a tracked object.
        ("- [`crates/`](crates) — Rust workspace", 1),
        # Another repository, reached by URL: not a path here.
        ("[docs](https://github.com/emu198x/docs)", 0),
        # A bare anchor into the same page.
        ("see [Building](#building)", 0),
        # `../` targets are REF's half of the surface, not this one.
        ("[x](../../knowledge/decisions/nes-clock-topology.md)", 0),
        # A path named in prose rather than linked. `RULES.md` cites globs and
        # placeholder paths this way, and neither is a link.
        ("The standard is `docs/testing-policy.md`, see `knowledge/*.md`", 0),
    ]

    # (file, target, expected verdict) — taken from the tree, except where noted.
    local_verdicts = [
        ("README.md", "docs/status/current-system-usability.md", "resolves"),
        # A directory resolves when the tree has anything in it.
        ("README.md", "crates", "resolves"),
        # `knowledge/chips/` is a local notebook, ignored on purpose.
        ("knowledge/index.md", "chips/zilog-z80.md", "ignored"),
        # The same notebook linked as a directory. Its `.gitignore` pattern
        # ends in a slash, so this resolves only when git is asked in the
        # directory form — and in a clean checkout the directory is absent,
        # which is where a filesystem answer would differ from git's.
        ("AGENTS.md", "knowledge/chips/", "ignored"),
        # The failure this half exists for: a repo-relative target that moved
        # to another repository, leaving a link that 404s on GitHub.
        ("README.md", "docs/testing-policy.md", "missing"),
        # Constructed — no page in the tree climbs out without a `../` prefix,
        # and an untested branch is one that has stopped being checked.
        ("README.md", "docs/../../PRINCIPLES.md", "outside"),
    ]

    failures = 0
    fake = ROOT / "crates" / "nonexistent" / "src" / "lib.rs"
    for source, expected in shape:
        got = len(references(source, comments_only=True))
        if got != expected:
            print(f"self-test FAILED: {source!r} — want {expected} refs, got {got}")
            failures += 1

    for source, expected in local_shape:
        got = len(local_links(source))
        if got != expected:
            print(f"self-test FAILED: {source!r} — want {expected} links, got {got}")
            failures += 1

    tracked = tracked_files()
    wanted = set()
    for name, ref, _ in (*verdicts, *local_verdicts):
        if (rel := inside_repo(ROOT / name, ref)) is not None:
            wanted.update((rel, f"{rel}/"))
    ignored = ignored_files(wanted - tracked)
    for name, ref, expected in verdicts:
        got = classify(ROOT / name, ref, tracked, ignored)
        if got != expected:
            print(f"self-test FAILED: {name} -> {ref} — want {expected}, got {got}")
            failures += 1

    dirs = tracked_dirs(tracked)
    for name, ref, expected in local_verdicts:
        got = classify_local(ROOT / name, ref, tracked, ignored, dirs)
        if got != expected:
            print(f"self-test FAILED: {name} -> {ref} — want {expected}, got {got}")
            failures += 1

    if failures:
        return 1
    cases = len(shape) + len(verdicts) + len(local_shape) + len(local_verdicts)
    print(f"self-test: {cases} cases pass")
    return 0


def main() -> int:
    if "--self-test" in sys.argv and self_test() != 0:
        return 1

    scanned = targets()
    text = {path: path.read_text(errors="replace") for path, _ in scanned}
    found = [
        (path, number, ref)
        for path, comments_only in scanned
        for number, ref in references(text[path], comments_only)
    ]
    local = [
        (path, number, ref)
        for path, comments_only in scanned
        if not comments_only
        for number, ref in local_links(text[path])
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
    for path, _, ref in local:
        if (rel := inside_repo(path, ref)) is not None:
            wanted.update((rel, f"{rel}/"))
    ignored = ignored_files(wanted - tracked)
    dirs = tracked_dirs(tracked)

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

    for path, number, ref in local:
        rel = path.relative_to(ROOT)
        verdict = classify_local(path, ref, tracked, ignored, dirs)
        counts[verdict] += 1
        if verdict != "missing":
            continue
        # Where a file of that name does exist, the link is a stale path
        # rather than a stale target, and naming it saves the search.
        base = Path(ref).name
        elsewhere = sorted(t for t in tracked | dirs if Path(t).name == base)
        hint = (
            f"nothing at {inside_repo(path, ref)} — did you mean {elsewhere[0]}?"
            if len(elsewhere) == 1
            else f"nothing at {inside_repo(path, ref)} — candidates: {elsewhere}"
            if elsewhere
            else f"nothing at {inside_repo(path, ref)}, and no {base} anywhere "
            "in this repo — has it moved to another repository?"
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
