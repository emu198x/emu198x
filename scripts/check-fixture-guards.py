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

# Any return, whatever it yields. The first version of this check matched
# only `return;`, `return true;` and `return false;`, and missed 86 guards —
# more than the 54 the sweep it was written to protect had found. The
# dominant shape here is a helper that reports the absence and returns
# `None`, which its caller discards with `else { return }`:
#
#     fn load_kickstart() -> Option<Vec<u8>> {
#         if !path.exists() {
#             eprintln!("skipping: Kickstart 1.3 ROM missing at {}", ...);
#             return None;
#         }
#
# The `eprintln!` and the bail sit together *inside the helper*, so matching
# any return catches the split form without following the call graph.
RETURNS = re.compile(r"^\s*return\b[^;]*;", re.M)

# `record()` is the sanctioned non-returning half: a helper that records the
# skip and then returns `None` has done its job, and the tally counts it. A
# window mentioning the skip crate is therefore not a silent guard — without
# this, the fix for every offender would trip the check that demanded it.
RECORDS = re.compile(r"emu198x_test_skip|skip!\(|record\(")

# A bail that is the function's trailing expression rather than a `return`
# statement. `kickstart_disk_path()` ends `eprintln!(...); None }` — no
# `return` anywhere — and that shape kept five A1000 tests passing in silence
# even after their sibling guards were converted.
TRAILING_BAIL = re.compile(
    r"\A\s*(?://[^\n]*\n\s*)*(None|Ok\(\(\)\)|false|true)\s*\n?\s*\}"
)

# How far after the report a return still counts as part of the same guard.
# Characters rather than lines, because the report itself may be wrapped over
# several and a line budget then measures formatting rather than distance.
TAIL_CHARS = 300


def is_test_code(path: Path, text: str) -> bool:
    return "/tests/" in str(path) or "#[cfg(test)]" in text


def macro_call(text: str, start: int) -> tuple[str, int]:
    """The whole `eprintln!(...)` invocation at `start`, and where it ends.

    Matching the trigger line alone missed every guard rustfmt had wrapped —
    and it wraps exactly the ones that interpolate a path, which is most of
    them. `eprintln!(` then sits on a line with no absence word on it and the
    message sits on a line with no `eprintln!` on it, so neither half matches.
    """
    open_paren = text.find("(", start)
    if open_paren == -1:
        return text[start:start + 200], start + 200
    depth = 0
    for index in range(open_paren, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return text[start : index + 1], index + 1
    return text[start:], len(text)


def offenders(root: Path) -> list[tuple[Path, int, str]]:
    found = []
    for path in sorted(root.rglob("*.rs")):
        text = path.read_text(errors="ignore")
        if not is_test_code(path, text):
            continue
        for match in re.finditer(r"eprintln!", text):
            call, end = macro_call(text, match.start())
            # The absence must be reported by the message itself. Scanning a
            # line window instead picks up the word from a neighbouring
            # comment: the Dragon golden helper's update-mode branch writes
            # the file and returns, three lines above a comment explaining
            # that a *missing* golden used to be a silent pass.
            if not REPORTS_ABSENCE.search(call):
                continue
            tail = text[end : end + TAIL_CHARS]
            bails = RETURNS.search(tail) or TRAILING_BAIL.match(tail.lstrip(" ;\n\t"))
            if bails and not RECORDS.search(call + tail):
                line = text.count("\n", 0, match.start()) + 1
                found.append((path, line, " ".join(call.split())[:100]))
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


HELPER = '''
fn load_kickstart() -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read ROM"))
}

#[test]
fn silent_through_a_helper() {
    let Some(rom) = load_kickstart() else { return };
    assert!(!rom.is_empty());
}
'''

RESULT = '''
#[test]
fn silent_returning_ok() -> Result<(), Box<dyn Error>> {
    let Some(dir) = rom_dir() else {
        eprintln!("skip: no C64 ROM dir");
        return Ok(());
    };
    Ok(())
}
'''

RECORDED = '''
fn load_kickstart() -> Option<Vec<u8>> {
    if !path.exists() {
        emu198x_test_skip::record(&format!("Kickstart missing at {}", path.display()));
        return None;
    }
    Some(std::fs::read(&path).expect("read ROM"))
}
'''


# rustfmt wraps `eprintln!` once the message interpolates a path, which is
# most fixture guards. The message then sits on a line with no `eprintln!`
# and the macro on a line with no absence word — invisible to a check that
# reads one line. Eight live guards were hiding behind exactly this.
WRAPPED = '''
fn load_rom() -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!(
            "skipping: A1000 bootstrap ROM missing at {}",
            path.display()
        );
        return None;
    }
    Some(std::fs::read(&path).expect("read ROM"))
}
'''

# ...and the over-correction to guard against. Widening the search to a line
# window instead of the macro call flags this: the message reports a *write*,
# but a comment three lines down explains that a missing golden used to be a
# silent pass, and the window cannot tell the two apart.
UPDATE_MODE = '''
fn compare_or_update(name: &str, png: &[u8]) {
    if update_mode() {
        std::fs::write(&golden_path, png).expect("write golden");
        eprintln!("wrote Dragon golden at {}", golden_path.display());
        return;
    }
    // Write it, then fail. Returning here instead made a missing golden a
    // silent pass: delete the file and the test goes green having compared
    // nothing.
    assert!(golden_path.exists(), "golden missing");
}
'''

# A helper whose bail is the trailing expression, with no `return` in sight.
TRAILING = '''
fn kickstart_disk_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_A1000_DISK") {
        return Some(PathBuf::from(path));
    }
    eprintln!("skipping: A1000 Kickstart disk not found; set EMU198X_A1000_DISK");
    None
}
'''

def self_test(tmp: Path) -> None:
    """Prove the detector still detects, then prove it still discriminates."""
    cases = [
        ("good.rs", GOOD, 0),
        ("bad.rs", BAD, 1),
        ("noisy.rs", NOISY, 0),
        # The three the first version of this checker walked straight past.
        ("helper.rs", HELPER, 1),
        ("result.rs", RESULT, 1),
        # And the fix for them, which must not itself register as a guard.
        ("recorded.rs", RECORDED, 0),
        # The formatting the line-based check could not see.
        ("wrapped.rs", WRAPPED, 1),
        # ...without flagging a write-and-return whose *comment* says missing.
        ("update_mode.rs", UPDATE_MODE, 0),
        # A bail with no `return` in it at all.
        ("trailing.rs", TRAILING, 1),
    ]
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
    print(
        "self-test passed: detects bare, helper, Result-returning, "
        "rustfmt-wrapped and trailing-expression guards; ignores an ordinary eprintln!, a recorded "
        "skip, and a write-and-return whose comment mentions a missing file"
    )


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
