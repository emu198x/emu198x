#!/usr/bin/env python3
"""Fail if a workflow installs apt packages other than through the shared script.

An unreachable Ubuntu mirror makes `apt-get` retry with its default
timeouts. In practice that is twenty to thirty minutes per job, before any
of this repository's code is involved, and it looks like a hang rather than
a failure — so the response is to cancel and re-run, which is a coin toss
against whichever mirror the next runner draws.

Every `apt-get install` therefore runs under a hard `timeout`, with
explicit acquire timeouts and a retry cap. This check exists because the
first fix covered `ci.yml` and missed `maintain-release.yml` and the four
sites in `nightly-accuracy.yml` — the release workflow then hung on exactly
the mirror the fix was written for.

Bounding was still not enough: a mirror that answers slowly fails every
attempt in turn, because apt only moves to the next mirror on an error.
`scripts/ci/apt-install.sh` puts a different mirror first on each retry, so
workflows install through it and nothing else. This check fails on any
`apt-get install` in a workflow, and on the script if it loses its bounds or
its mirror rotation.

## Self-test

`--self-test` proves the detector still flags an unbounded step and still
accepts a bounded one. There is no CI job over the scripts' own tests, so a
checker that stopped detecting would report clean and be believed.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# An apt-get *command*: at the start of a line or after a shell separator,
# `run:` or a shell keyword, optionally under sudo and `timeout N`. Text inside
# an `echo` message is not a command and does not match.
COMMAND = re.compile(
    r"(?:^|[;&|]|\brun:)\s*(?:(?:if|then|do|else|!)\s+)*(?:sudo\s+)?"
    r"(?P<timeout>timeout\s+\d+\s+)?apt-get\b(?P<args>[^;&|]*)"
)

BAD = """
      - name: Install deps
        run: |
          sudo apt-get update
          sudo apt-get install -y pkg-config
"""

GOOD = """
      - name: Install deps
        run: |
          for attempt in 1 2 3; do
            sudo timeout 120 apt-get update || true
            if sudo timeout 180 apt-get install -y pkg-config; then exit 0; fi
          done
          exit 1
"""

# What a workflow does instead.
SCRIPT_CALL = """
      - name: Install deps
        run: bash scripts/ci/apt-install.sh pkg-config libasound2-dev
"""

# A one-line step: the install follows `run:` and `&&`, not the line start.
INLINE = """
      - name: Install deps
        run: sudo apt-get update && sudo apt-get install -y pkg-config
"""

# Words in a message are not a command.
MESSAGE = """
          echo "::warning::apt-get install attempt ${attempt} failed; retrying"
"""


SCRIPT = REPO / "scripts" / "ci" / "apt-install.sh"


def installs(text: str, *, bounded_ok: bool) -> list[str]:
    """`apt-get install` command lines; with `bounded_ok`, only unbounded ones."""
    found = []
    for line in text.splitlines():
        if line.lstrip().startswith("#"):
            continue
        for command in COMMAND.finditer(line):
            if re.search(r"\binstall\b", command["args"]) and not (
                bounded_ok and command["timeout"]
            ):
                found.append(line.strip())
                break
    return found


def unbounded(text: str) -> list[str]:
    return installs(text, bounded_ok=True)


def raw(text: str) -> list[str]:
    return installs(text, bounded_ok=False)


def self_test() -> None:
    for name, check, body, expected in (
        ("bad", unbounded, BAD, 1),
        ("good", unbounded, GOOD, 0),
        ("inline", unbounded, INLINE, 1),
        ("message", unbounded, MESSAGE, 0),
        ("bounded but hand-rolled", raw, GOOD, 1),
        ("script call", raw, SCRIPT_CALL, 0),
        ("message in a workflow", raw, MESSAGE, 0),
    ):
        found = len(check(body))
        if found != expected:
            raise SystemExit(
                f"self-test FAILED: {name} sample should yield {expected}, got {found}. "
                "The detector has stopped detecting; fix it before trusting a pass."
            )
    print(
        "self-test passed: flags unbounded apt-get installs, inline or not, and any "
        "hand-rolled install in a workflow; accepts the script call and messages"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()

    problems = []
    for path in sorted((REPO / ".github" / "workflows").glob("*.yml")):
        for line in raw(path.read_text()):
            problems.append((path.relative_to(REPO), line))
    script = SCRIPT.read_text()
    for line in unbounded(script):
        problems.append((SCRIPT.relative_to(REPO), line))
    if "apt-mirrors.txt" not in script or "fallbacks=" not in script:
        problems.append((SCRIPT.relative_to(REPO), "the mirror rotation is gone"))

    if not problems:
        print("every apt install goes through scripts/ci/apt-install.sh, which is bounded and rotates mirrors")
        return 0

    print(f"{len(problems)} problem(s):\n")
    for path, line in problems:
        print(f"  {path}\n      {line}")
    print(
        "\nA slow or unreachable mirror turns a raw install into a 20-30 minute hang, or "
        "a failed job.\nInstall with `bash scripts/ci/apt-install.sh PACKAGE...` instead."
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
