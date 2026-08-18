#!/usr/bin/env python3
"""Print one version's section of the changelog.

Used to put the release notes in the release PR's description. release-plz did
this, and it is the difference between a reviewer reading what is being
released and scrolling a hundred-file diff hoping to find CHANGELOG.md among
the manifests.

Usage:
    changelog-section.py CHANGELOG.md 0.4.0
"""

import pathlib
import re
import sys


def section(text: str, version: str) -> str:
    """The body under `## [version]`, up to the next version heading."""
    # The heading carries a date, so match the version and take the rest of
    # the line with it.
    start = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n", text, re.M)
    if not start:
        return ""
    rest = text[start.end() :]
    end = re.search(r"^## \[", rest, re.M)
    return (rest[: end.start()] if end else rest).strip()


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: changelog-section.py <changelog> <version>")
    path, version = pathlib.Path(sys.argv[1]), sys.argv[2]
    print(section(path.read_text(), version))


if __name__ == "__main__":
    main()
