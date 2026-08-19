#!/usr/bin/env python3
"""Fail if a test guard reports a missing fixture and returns.

libtest prints `ok` for a test that returns early. So this:

    if !rom.exists() {
        eprintln!("48K ROM not found at {}", rom.display());
        return;
    }

is indistinguishable, in the run output and in CI, from a test that ran and
passed. It is the shape `emu198x-test-skip` exists to remove.

The cost is not theoretical. The Dragon golden-frame test reported `ok` in
CI for nearly three months while comparing nothing. `goldens.rs` reported
eight passes on a runner with no ROM present. `emu198x-spectrum`'s MCP and
script-runner tests are not even `#[ignore]`d — they ran on every push and
exercised nothing. #1011 swept 54 of these across 39 files; this exists so
the next copied guard does not quietly restore the class.

## The fix a failure is asking for

    if !rom.exists() {
        emu198x_test_skip::skip!("48K ROM not staged at {}", rom.display());
    }

`skip!` returns from the test, records the skip so the tally counts it, and
panics under `EMU198X_STRICT_FIXTURES` — so a job that provisioned the
fixture cannot quietly run less than it claims. Where the caller needs a
value rather than a return, `emu198x_test_skip::record` is the
non-returning half.

## Self-test

`--self-test` runs the detector against known-good and known-bad samples
before scanning. A checker that has stopped detecting is the same failure
as the guards it looks for, and this repository has no CI job that runs the
scripts' own tests — so the check carries its own.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# Words that mark an `eprintln!` as reporting an absent fixture rather than
# printing diagnostics. Kept deliberately narrow: the Amiga trace tests
# print a great deal that is not a guard.
REPORTS_ABSENCE = re.compile(
    r"not found|missing|not present|skipping|skip:|not staged|not set", re.I
)

# A bare return, or one yielding a bool — helpers such as `skip_if_missing`
# return a flag their caller branches on, and those hid failures too.
RETURNS = re.compile(r"^\s*return(\s+(true|false))?\s*;", re.M)

# How far after the `eprintln!` a return still counts as the same guard.
WINDOW = 7


def is_test_code(path: Path, text: str) -> bool:
    return "/tests/" in str(path) or "#[cfg(test)]" in text


def offenders(root: Path) -> list[tuple[Path, int, str]]:
    found = []
    for path in sorted(root.rglob("*.rs")):
        text = path.read_text(errors="ignore")
        if not is_test_code(path, text):
            continue
        lines = text.splitlines()
        for index, line in enumerate(lines):
            if "eprintln!" not in line or not REPORTS_ABSENCE.search(line):
                continue
            if RETURNS.search("\n".join(lines[index : index + WINDOW])):
                found.append((path, index + 1, line.strip()))
    return found


GOOD = '''
#[test]
fn honest() {
    if !rom.exists() {
        emu198x_test_skip::skip!("ROM not staged at {}", rom.display());
    }
    assert!(true);
}
'''

BAD = '''
#[test]
fn silent() {
    if !rom.exists() {
        eprintln!("ROM not found at {}", rom.display());
        return;
    }
    assert!(true);
}
'''

NOISY = '''
#[test]
fn prints_but_does_not_guard() {
    eprintln!("ExecBase is missing from the list, continuing anyway");
    assert!(walk_the_list());
}
'''


def self_test(tmp: Path) -> None:
    """Prove the detector still detects, then prove it still discriminates."""
    cases = [("good.rs", GOOD, 0), ("bad.rs", BAD, 1), ("noisy.rs", NOISY, 0)]
    for name, body, expected in cases:
        sample = tmp / "tests"
        sample.mkdir(parents=True, exist_ok=True)
        target = sample / name
        target.write_text(body)
        hits = len(offenders(tmp))
        target.unlink()
        if hits != expected:
            raise SystemExit(
                f"self-test FAILED: {name} should yield {expected} hit(s), got {hits}. "
                "The detector has stopped detecting; fix it before trusting a pass."
            )
    print("self-test passed: detects the guard, ignores an ordinary eprintln!")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path, default=REPO / "crates")
    args = parser.parse_args()

    if args.self_test:
        import tempfile

        with tempfile.TemporaryDirectory(prefix="fixture-guard-selftest-") as tmp:
            self_test(Path(tmp))

    found = offenders(args.root)
    if not found:
        print("no fixture guard reports a missing fixture and returns")
        return 0

    print(f"{len(found)} fixture guard(s) pass in silence:\n")
    for path, line, text in found:
        print(f"  {path.relative_to(REPO)}:{line}")
        print(f"      {text}")
    print(
        "\nlibtest reports an early return as `ok`, so each of these claims a pass "
        "for a test that did not run.\n"
        "Replace with `emu198x_test_skip::skip!(...)`, or `record(...)` where the "
        "caller needs a value. See knowledge/decisions/"
        "a-gate-nobody-runs-is-a-silent-gate.md."
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
