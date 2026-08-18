#!/usr/bin/env python3
"""Render a collected evidence ledger as a GitHub step summary.

Reads the JSON `collect_evidence.py` writes and prints Markdown. Kept
separate from collection so a failing suite still summarises: the
collector writes its ledger before it judges anything, and this runs with
`if: always()`.

What it is for is the same thing the ledger is for — a green tick says a
run finished, not what it covered. The tables below say what did not run
and, where the test bothered to state it, why.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# Enough rows to act on without burying the summary. The remainder is
# counted rather than dropped: a silent truncation reads as "that was all
# of it".
TOP_REASONS = 15


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: summarise_evidence.py <evidence.json>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.exists():
        # The collector died before writing. Say so rather than print an
        # empty summary that reads like a clean run.
        print("## Test evidence\n\nNo ledger was written — the collector did not finish.")
        return 0

    data = json.loads(path.read_text())
    packages = data["packages"]
    total = {
        key: sum(p[key] for p in packages.values())
        for key in ("passed", "failed", "ignored", "skipped", "unexplained_ignored")
    }

    out: list[str] = ["## Test evidence", ""]
    out.append(
        f"**{total['passed']} passed** · {total['failed']} failed · "
        f"{total['ignored']} ignored · {total['skipped']} skipped for a missing fixture"
    )
    out.append("")

    did_not_run = total["ignored"] + total["skipped"]
    if did_not_run:
        share = did_not_run / (total["passed"] + did_not_run) * 100
        out.append(
            f"{did_not_run} tests did not run — {share:.1f}% of the suite. "
            "A green tick does not cover these."
        )
        out.append("")

    reasons = data.get("ignored_reasons", [])
    if reasons:
        out += ["### Why tests did not run", "", "| Tests | Stated reason |", "|---|---|"]
        for entry in reasons[:TOP_REASONS]:
            reason = entry["reason"].replace("|", "\\|")
            out.append(f"| {entry['tests']} | {reason} |")
        remainder = sum(e["tests"] for e in reasons[TOP_REASONS:])
        if remainder:
            out.append(
                f"| {remainder} | _…across {len(reasons) - TOP_REASONS} further reasons_ |"
            )
        out.append("")

    if total["unexplained_ignored"]:
        out.append(
            f"**{total['unexplained_ignored']} ignored tests state no reason at all.** "
            "Those are not a queue of work — they are a silence."
        )
        out.append("")

    systems = data.get("systems", [])
    if systems:
        out += [
            "### Evidence by machine",
            "",
            "`own` counts tests in crates reachable from this machine alone. "
            "`shared` counts crates it has in common with other machines, which "
            "cannot tell them apart.",
            "",
            "| Machine | Own passed | Own not run | Shared passed |",
            "|---|---|---|---|",
        ]
        for system in sorted(systems, key=lambda s: -s["own"]["passed"]):
            own, shared = system["own"], system["shared"]
            not_run = own["ignored"] + own["skipped"]
            name = system["machine_id"]
            if system["shares_crate_with"]:
                name += " ⚠️"
            out.append(
                f"| {name} | {own['passed']} | {not_run} | {shared['passed']} |"
            )
        if any(s["shares_crate_with"] for s in systems):
            out += [
                "",
                "⚠️ shares a shipping crate with another machine, so neither "
                "machine's own evidence can be told from the other's.",
            ]
        out.append("")

    print("\n".join(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
