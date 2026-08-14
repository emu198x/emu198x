#!/usr/bin/env python3
"""Remove a version's section from CHANGELOG.md so it can be regenerated.

`git-cliff --prepend` inserts unconditionally, so running it twice for the same
version produces two identical sections. The release workflow runs on every push
to `main`, so it *will* run many times for one version. Stripping first makes
the pair idempotent: the section is replaced rather than repeated.

Regenerating the whole file instead would be idempotent for free, but it would
also destroy the hand-written v0.1.0 section — the "What works" prose that no
commit history can reconstruct — so only the version under construction is
touched.

    scripts/changelog-section-reset.py CHANGELOG.md 0.2.2
"""

from __future__ import annotations

import pathlib
import re
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2

    path = pathlib.Path(sys.argv[1])
    version = sys.argv[2]

    if not path.exists():
        print(f"{path} does not exist; nothing to strip")
        return 0

    text = path.read_text()
    # From this version's heading up to the next version heading, or the end of
    # the file if it is the newest section.
    pattern = re.compile(
        r"^## \[" + re.escape(version) + r"\].*?(?=^## \[|\Z)",
        re.MULTILINE | re.DOTALL,
    )
    new_text, count = pattern.subn("", text)

    if count:
        path.write_text(new_text)
    print(f"stripped {count} existing section(s) for {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
