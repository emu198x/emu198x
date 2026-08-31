#!/usr/bin/env python3
"""Reject crates.io API probes that omit the required identifying user agent."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


CRATES_API = re.compile(r"curl(?P<arguments>.*?)https://crates\.io/api/", re.DOTALL)
USER_AGENT = re.compile(r"(?:--user-agent|-A)(?:\s|=)")


def missing_user_agents(text: str) -> int:
    return sum(not USER_AGENT.search(match.group("arguments")) for match in CRATES_API.finditer(text))


def self_test() -> None:
    assert missing_user_agents('curl "https://crates.io/api/v1/crates/a/1"') == 1
    assert missing_user_agents('curl --user-agent "project (url)" "https://crates.io/api/v1/crates/a/1"') == 0
    assert missing_user_agents('curl -A agent "https://crates.io/api/v1/crates/a/1"') == 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()

    workflow = Path(".github/workflows/publish-crates.yml")
    missing = missing_user_agents(workflow.read_text())
    if missing:
        print(f"{workflow}: {missing} crates.io API curl call(s) omit --user-agent")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
