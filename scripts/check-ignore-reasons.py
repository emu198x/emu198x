#!/usr/bin/env python3
"""Checks that every `#[ignore]` says which *kind* of ignored it is.

`#[ignore]` carries three incompatible meanings in this workspace, and a
batch `--ignored` sweep sees one red bar for all of them:

  FIXTURE           needs data that is not in the repo. Fails here, passes
                    in the nightly accuracy run once the corpus is staged.
  DIAGNOSTIC        a hand-run investigation tool, not a gate. Not expected
                    to be part of anyone's pass/fail reading.
  SLOW              passes; too expensive to run per-PR.
  KNOWN DIVERGENCE  deliberately red. Expected to fail *everywhere*, until
  KNOWN LIMITATION  someone fixes the modelling or the harness.

Without the prefix you cannot tell a regression from an unset environment
variable without opening every test. That is how #1226 stayed red for two
weeks: it was a real failure sitting in a crowd of expected ones.

The two KNOWN forms must also carry an anchor — a `#NNN` issue or a
`knowledge/decisions/*.md` path — because their whole purpose is that
somebody comes back to them. Closing the issue should surface the test.

## The backlog

179 attributes were bare when this check landed, and 155 of those state no
intent anywhere, so nobody can classify them without reading the test and
guessing. Guessed intent recorded as fact is worse than none, so they are
listed in `scripts/ignore-reasons-backlog.txt` instead. The list may shrink,
never grow: a new bare `#[ignore]` fails, and an entry naming a test that no
longer exists fails too, so the file cannot rot into sediment.

Run `--self-test` to check the detector itself. A checker that has stopped
detecting is the same failure as the tests it looks for.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

BACKLOG = Path(__file__).parent / "ignore-reasons-backlog.txt"

PREFIXES = ("FIXTURE", "DIAGNOSTIC", "SLOW", "KNOWN DIVERGENCE", "KNOWN LIMITATION")
NEEDS_ANCHOR = ("KNOWN DIVERGENCE", "KNOWN LIMITATION")
ANCHOR = re.compile(r"#\d+|knowledge/decisions/[\w-]+\.md")

# The attribute, its optional reason, and enough trailing text to name the
# test it guards. Matching the attribute rather than scanning line by line
# is deliberate: rustfmt wraps a long reason across lines, and a line-based
# scan misses every wrapped one — the blind spot that made #1227.
ATTR = re.compile(
    r"#\[ignore(?:\s*=\s*\"((?:[^\"\\]|\\.)*)\")?\s*\]([^{]{0,400})",
    re.S,
)
FN = re.compile(r"fn\s+([a-zA-Z0-9_]+)")


def strip_comments(text: str) -> str:
    """Blanks out comments, leaving code (and string literals) intact.

    `#[ignore]` appears in prose far more often than you would guess — doc
    comments explaining why a sibling test is ignored, section banners,
    module headers. A scan that counts those reports attributes that do not
    exist: it found 178 bare ones here, of which 63 were sentences.

    String-aware on purpose. A reason may contain `//` (a URL, a path), and
    a naive strip would cut the literal in half and corrupt the parse.
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        if ch == '"':
            out.append(ch)
            i += 1
            while i < n:
                out.append(text[i])
                if text[i] == "\\":
                    if i + 1 < n:
                        out.append(text[i + 1])
                        i += 2
                        continue
                elif text[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        if text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue
        if text.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    if text[i] == "\n":
                        out.append("\n")
                    i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def normalise(reason: str) -> str:
    """Collapses a rustfmt-wrapped reason onto one line."""
    return re.sub(r"\s*\\?\s*\n\s*", " ", reason).strip()


def scan(text: str, path: str) -> list[tuple[str, str, str | None]]:
    """Yields (path::test, kind, message) for every `#[ignore]` in `text`."""
    found = []
    for match in ATTR.finditer(strip_comments(text)):
        reason, trailing = match.group(1), match.group(2)
        name = FN.search(trailing)
        ident = f"{path}::{name.group(1) if name else '?'}"

        if reason is None:
            found.append((ident, "bare", None))
            continue

        text_reason = normalise(reason)
        prefix = next((p for p in PREFIXES if text_reason.startswith(p)), None)
        if prefix is None:
            found.append((ident, "unprefixed", text_reason[:80]))
        elif prefix in NEEDS_ANCHOR and not ANCHOR.search(text_reason):
            found.append((ident, "no-anchor", text_reason[:80]))
    return found


def repo_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*.rs"], capture_output=True, text=True, check=True
    )
    return out.stdout.split()


def load_backlog() -> set[str]:
    if not BACKLOG.exists():
        return set()
    return {
        line.strip()
        for line in BACKLOG.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    }


def self_test() -> int:
    """Samples lifted from the tree, not written to match the regex.

    Every sample in #1227's self-test was written inline by the same hand
    that wrote the check, so it could only ever confirm the author's model.
    These are real shapes: a rustfmt-wrapped reason, an attribute stacked
    under `#[test]`, one inside a `mod tests` block.
    """
    cases = [
        # A wrapped reason — the shape a line-based scan cannot see.
        (
            '#[test]\n#[ignore = "FIXTURE: needs EMU198X_SPECTRUM_48K_ROM to run \\\n'
            '            this harness"]\nfn a() {}',
            [],
        ),
        # Bare.
        ("#[test]\n#[ignore]\nfn b() {}", [("x.rs::b", "bare")]),
        # A reason with no category at all.
        (
            '#[test]\n#[ignore = "requires a local ROM"]\nfn c() {}',
            [("x.rs::c", "unprefixed")],
        ),
        # Deliberately red, but nothing to come back to.
        (
            '#[test]\n#[ignore = "KNOWN DIVERGENCE: the mask caps at 5"]\nfn d() {}',
            [("x.rs::d", "no-anchor")],
        ),
        # Deliberately red with an issue, and with a decision record.
        (
            '#[ignore = "KNOWN DIVERGENCE (#856): the mask caps at 5"]\nfn e() {}',
            [],
        ),
        (
            '#[ignore = "KNOWN LIMITATION: see \\\n'
            '            knowledge/decisions/io-contention-is-a-count-not-a-level.md"]\n'
            "fn f() {}",
            [],
        ),
        # Prose, not an attribute. Lifted from boot_invariants.rs, which
        # explains in a module comment why its siblings are ignored — 63 of
        # the 178 "bare" attributes the first version of this check found
        # were sentences like these.
        ("//! backed invariants are `#[ignore]`'d and resolve assets from\n", []),
        ("/// `#[ignore]`'d only because it needs the local 48K ROM.\nfn h() {}", []),
        ("// ROM-backed — `#[ignore]`'d; resolve assets under ~/.emu198x/\n", []),
        ("/* a block comment mentioning #[ignore] */\n", []),
        # A reason containing `//`. A naive comment strip cuts the literal
        # in half and the attribute stops parsing.
        (
            '#[ignore = "FIXTURE: see https://example.invalid/corpus"]\nfn i() {}',
            [],
        ),
        # Inside a `mod tests` block, indented — the in-src unit-test shape.
        (
            'mod tests {\n    #[test]\n    #[ignore = "SLOW: full corpus"]\n'
            "    fn g() {}\n}",
            [],
        ),
    ]
    failures = 0
    for source, expected in cases:
        got = [(i, k) for i, k, _ in scan(source, "x.rs")]
        if got != expected:
            print(f"self-test FAILED\n  source: {source!r}\n  want {expected}, got {got}")
            failures += 1
    if failures:
        print(f"{failures} self-test case(s) failed")
        return 1
    print(f"self-test: {len(cases)} cases pass")
    return 0


def main() -> int:
    # Matches the sibling checks: the self-test runs first, then the scan
    # falls through, so one CI step covers both the detector and the tree.
    if "--self-test" in sys.argv and self_test() != 0:
        return 1

    backlog = load_backlog()
    problems: list[str] = []
    seen_bare: set[str] = set()

    for path in repo_files():
        text = Path(path).read_text(errors="replace")
        if "#[ignore" not in text:
            continue
        for ident, kind, detail in scan(text, path):
            if kind == "bare":
                seen_bare.add(ident)
                if ident not in backlog:
                    problems.append(
                        f"{ident}\n    bare #[ignore] — say which kind: "
                        f"{', '.join(PREFIXES)}"
                    )
            elif kind == "unprefixed":
                problems.append(
                    f"{ident}\n    reason has no category prefix: {detail!r}\n"
                    f"    expected one of: {', '.join(PREFIXES)}"
                )
            elif kind == "no-anchor":
                problems.append(
                    f"{ident}\n    deliberately-red without an anchor: {detail!r}\n"
                    "    add a #NNN issue or a knowledge/decisions/*.md path"
                )

    stale = sorted(backlog - seen_bare)
    for ident in stale:
        problems.append(
            f"{ident}\n    listed in {BACKLOG.name} but no longer a bare "
            "#[ignore] — delete the line"
        )

    if problems:
        print(f"{len(problems)} problem(s):\n")
        for problem in problems:
            print(f"  {problem}\n")
        print(f"backlog: {len(seen_bare)}/{len(backlog)} known bare attributes remain")
        return 1

    print(f"ignore reasons OK — {len(seen_bare)} in the backlog, none new")
    return 0


if __name__ == "__main__":
    sys.exit(main())
