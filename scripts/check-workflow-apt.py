#!/usr/bin/env python3
"""Fail if a workflow installs packages without bounding apt.

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

INSTALL = re.compile(r"^\s*(sudo\s+)?apt-get\b.*\binstall\b", re.M)
BOUNDED = re.compile(r"\btimeout\s+\d+\s+apt-get\b")

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


def unbounded(text: str) -> list[str]:
    return [
        line.strip()
        for line in text.splitlines()
        if INSTALL.match(line) and not BOUNDED.search(line)
    ]


def self_test() -> None:
    for name, body, expected in (("bad", BAD, 1), ("good", GOOD, 0)):
        found = len(unbounded(body))
        if found != expected:
            raise SystemExit(
                f"self-test FAILED: {name} sample should yield {expected}, got {found}. "
                "The detector has stopped detecting; fix it before trusting a pass."
            )
    print("self-test passed: flags an unbounded apt-get install, accepts a bounded one")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()

    problems = []
    for path in sorted((REPO / ".github" / "workflows").glob("*.yml")):
        for line in unbounded(path.read_text()):
            problems.append((path.relative_to(REPO), line))

    if not problems:
        print("every apt-get install in a workflow is bounded and retried")
        return 0

    print(f"{len(problems)} unbounded apt-get install(s):\n")
    for path, line in problems:
        print(f"  {path}\n      {line}")
    print(
        "\nAn unreachable mirror turns these into a 20-30 minute hang that reads as a "
        "stuck job.\nWrap in `timeout` with a retry loop — see the pattern in ci.yml."
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
