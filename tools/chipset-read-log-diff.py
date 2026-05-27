#!/usr/bin/env python3
"""Diff two MCP `chipset_read_log` outputs and surface differences.

Use case: capture the chipset_read_log from a working boot (e.g.
A500+ + KS 2.04 + WB 2.1) and the broken one (A1200 + KS 3.1) and
diff them to find offsets that the broken boot reads but the working
boot doesn't — strong candidates for AGA-only register reads we
mishandle.

Inputs are JSONL files (as produced by piping the MCP server into
a file). The script looks for a `chipset_read_log` tool response in
each, extracts its `entries` + `offset_summary`, and reports:

  - offsets present in one log but missing in the other
  - offsets present in both but with diverging value distributions

Usage:
    chipset-read-log-diff.py <baseline.jsonl> <subject.jsonl> [--id ID]

If `--id` is omitted, the script picks the first chipset_read_log
response in each file. Pass `--id 5` to target a specific
JSON-RPC response ID.
"""

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


def load_log(path: Path, target_id: int | None) -> dict:
    """Pull the chipset_read_log payload out of a JSONL transcript."""
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if "result" not in obj:
                continue
            if target_id is not None and obj.get("id") != target_id:
                continue
            content = obj["result"].get("content") or []
            if not content:
                continue
            text = content[0].get("text", "")
            try:
                body = json.loads(text)
            except json.JSONDecodeError:
                continue
            # `chipset_read_log` payloads carry offset_summary rows
            # keyed `reads`; `chipset_write_log` uses `writes`. Both
            # tools share the `offset_summary` + `entries` shape, so
            # check for the `reads` key before accepting.
            summary = body.get("offset_summary")
            entries = body.get("entries")
            if summary is None or entries is None:
                continue
            if summary and "reads" not in summary[0]:
                continue
            return body
    raise SystemExit(f"no chipset_read_log payload in {path}")


def offset_to_int(s: str) -> int:
    """Parse `$xxxx` / `0xxxxx` / plain hex into an int. `lstrip`
    would strip ALL of the characters in the prefix string, eating
    real `0` digits — use `removeprefix` so `$0000` parses as 0."""
    s = s.removeprefix("$").removeprefix("0x").removeprefix("0X")
    return int(s, 16) if s else 0


def summary_map(payload: dict) -> dict[int, int]:
    """offset_summary → {offset_int: read_count}."""
    out: dict[int, int] = {}
    for row in payload.get("offset_summary", []):
        off = offset_to_int(row["offset"])
        out[off] = row.get("reads", 0)
    return out


def value_counts(payload: dict) -> dict[int, Counter]:
    """Per-offset distribution of returned values."""
    out: dict[int, Counter] = {}
    for e in payload.get("entries", []):
        off = offset_to_int(e["offset"])
        val = offset_to_int(e["value"])
        out.setdefault(off, Counter())[val] += 1
    return out


REG_NAMES = {
    0x002: "DMACONR", 0x004: "VPOSR",   0x006: "VHPOSR",  0x008: "DSKDATR",
    0x00A: "JOY0DAT", 0x00C: "JOY1DAT", 0x00E: "CLXDAT",  0x010: "ADKCONR",
    0x012: "POT0DAT", 0x014: "POT1DAT", 0x016: "POTGOR",  0x018: "SERDATR",
    0x01A: "DSKBYTR", 0x01C: "INTENAR", 0x01E: "INTREQR",
    0x07C: "DENISEID",
    0x1FC: "FMODE",
}


def name(off: int) -> str:
    return REG_NAMES.get(off, "")


def fmt_off(off: int) -> str:
    n = name(off)
    return f"${off:04X}" + (f" {n}" if n else "")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("baseline", type=Path, help="JSONL transcript from the working boot")
    p.add_argument("subject", type=Path, help="JSONL transcript from the suspect boot")
    p.add_argument(
        "--id",
        type=int,
        default=None,
        help="JSON-RPC response id of the chipset_read_log call (default: first found)",
    )
    args = p.parse_args()

    base = load_log(args.baseline, args.id)
    subj = load_log(args.subject, args.id)

    base_sum = summary_map(base)
    subj_sum = summary_map(subj)
    base_vals = value_counts(base)
    subj_vals = value_counts(subj)

    only_in_subject = sorted(set(subj_sum) - set(base_sum))
    only_in_baseline = sorted(set(base_sum) - set(subj_sum))
    in_both = sorted(set(subj_sum) & set(base_sum))

    print(f"baseline: {args.baseline} — {sum(base_sum.values()):>7} reads, {len(base_sum)} offsets")
    print(f"subject : {args.subject} — {sum(subj_sum.values()):>7} reads, {len(subj_sum)} offsets")

    if only_in_subject:
        print(f"\n=== offsets read ONLY in subject ({len(only_in_subject)}) ===")
        print("    these are strong candidates for chipset-specific reads we may mishandle")
        for off in only_in_subject:
            vals = subj_vals.get(off, Counter())
            vstr = ", ".join(f"${v:04X}×{c}" for v, c in vals.most_common(4))
            print(f"  {fmt_off(off):<14}  {subj_sum[off]:>6} reads   values: {vstr}")

    if only_in_baseline:
        print(f"\n=== offsets read ONLY in baseline ({len(only_in_baseline)}) ===")
        for off in only_in_baseline:
            vals = base_vals.get(off, Counter())
            vstr = ", ".join(f"${v:04X}×{c}" for v, c in vals.most_common(4))
            print(f"  {fmt_off(off):<14}  {base_sum[off]:>6} reads   values: {vstr}")

    print(f"\n=== offsets in BOTH ({len(in_both)}) — value distribution diffs ===")
    for off in in_both:
        b = base_vals.get(off, Counter())
        s = subj_vals.get(off, Counter())
        b_set = set(b)
        s_set = set(s)
        if b_set == s_set:
            continue  # same value space — nothing interesting to flag here
        only_subj = s_set - b_set
        only_base = b_set - s_set
        if not (only_subj or only_base):
            continue
        print(f"  {fmt_off(off):<14}  baseline:{base_sum[off]:>5}r  subject:{subj_sum[off]:>5}r")
        if only_subj:
            extras = ", ".join(f"${v:04X}×{s[v]}" for v in sorted(only_subj)[:6])
            print(f"      +subject-only values: {extras}")
        if only_base:
            extras = ", ".join(f"${v:04X}×{b[v]}" for v in sorted(only_base)[:6])
            print(f"      +baseline-only values: {extras}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
