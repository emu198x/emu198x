#!/usr/bin/env python3
"""Compare every live D transfer, finish, busy release and Copper interval."""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compare(native: Path, reference: Path) -> dict:
    samples = [
        json.loads(line) for line in (native / "native.jsonl").read_text().splitlines()
    ]
    log = (native / "native-moves.log").read_text().splitlines()
    moves = [ast.literal_eval(line[5:]) for line in log if line.startswith("MOVE ")]
    writes = [json.loads(line[6:]) for line in log if line.startswith("WRITE ")]
    source = json.loads(reference.read_text())
    starts = [entry for entry in moves if entry[3] == 0x58]
    colours = [entry for entry in moves if entry[3:] == (0x180, 0xF00)]
    ref_starts = [
        entry
        for entry in source
        if entry["event"] == "move" and entry["val"] >> 16 == 0x58
    ]
    ref_colours = [
        entry
        for entry in source
        if entry["event"] == "move" and entry["val"] == 0x1800F00
    ]
    if not (len(starts) == len(colours) == len(ref_starts) == len(ref_colours) == 48):
        raise ValueError("missing or duplicate timing cases")
    if len(writes) != 1530 or any(
        entry["source"] != "Blitter" or entry["value"] != 0xFFFF for entry in writes
    ):
        raise ValueError("incorrect DMA write count, writer or data")
    if not samples or any(entry["guest_field"] < 9 for entry in samples):
        raise ValueError("guest did not reach its steady field counter")
    result = []
    for index, (start, colour, ref_start, ref_colour) in enumerate(
        zip(starts, colours, ref_starts, ref_colours, strict=True)
    ):
        end = starts[index + 1][0] if index < 47 else samples[-1]["tick"] // 2 + 1
        ref_end = ref_starts[index + 1]["cyc"] if index < 47 else source[-1]["cyc"] + 1
        window = [entry for entry in samples if start[0] <= entry["tick"] // 2 < end]
        dma = [entry for entry in writes if start[0] <= entry["cck"] < end]
        ref_dma = [
            entry
            for entry in source
            if entry["event"] == "write-D"
            and ref_start["cyc"] <= entry["cyc"] < ref_end
        ]
        expected_addresses = [
            0x40000 + 2 * word
            for word in range([1, 2, 4, 8, 16, 32, 64, 128][index % 8])
        ]
        if not (
            [entry["addr"] for entry in dma]
            == [entry["val"] for entry in ref_dma]
            == expected_addresses
        ):
            raise ValueError(f"incorrect transfer addresses in case {index}")
        # The size write is pipelined. The previous blit's finish can remain
        # asserted at MOVE service; require this blit's reset before its rise.
        reset = next(
            entry["tick"] // 2
            for entry in window
            if not entry["after"]["blitter"]["execution"]["finish_emitted"]
        )
        finish = next(
            entry["tick"] // 2
            for entry in window
            if entry["tick"] // 2 >= reset
            and entry["after"]["blitter"]["execution"]["finish_emitted"]
        )
        ref_finish = next(
            entry["cyc"]
            for entry in source
            if entry["event"] == "done-all"
            and ref_start["cyc"] <= entry["cyc"] < ref_end
        )
        busy = next(
            entry["tick"] // 2
            for entry in window
            if entry["tick"] // 2 > dma[-1]["cck"]
            and not entry["after"]["blitter"]["execution"]["busy_copper"]
        )
        ref_busy = next(
            entry["cyc"]
            for entry in source
            if entry["cyc"] > ref_dma[-1]["cyc"] and not entry["busy"]
        )
        result.append(
            {
                "index": index,
                "move_delta": (colour[0] - start[0])
                - (ref_colour["cyc"] - ref_start["cyc"]),
                "finish_delta": finish - start[0] - (ref_finish - ref_start["cyc"]),
                "busy_delta": busy - start[0] - (ref_busy - ref_start["cyc"]),
                "write_deltas": [
                    entry["cck"] - start[0] - (ref_entry["cyc"] - ref_start["cyc"])
                    for entry, ref_entry in zip(dma, ref_dma, strict=True)
                ],
            }
        )
    return {
        "rows": result,
        "input_sha256": {
            str(path): sha256(path)
            for path in [
                native / "native.jsonl",
                native / "native-moves.log",
                reference,
            ]
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    cases = {
        case: compare(
            args.native / case, args.reference / (case + "-retry") / "reference.json"
        )
        for case in ["bfd-ignore", "bfd-wait"]
    }
    rows = [row for case in cases.values() for row in case["rows"]]
    failing = [
        row
        for row in rows
        if row["move_delta"]
        or row["finish_delta"]
        or row["busy_delta"]
        or any(row["write_deltas"])
    ]
    report = {
        "boundary": "registered software-reference trace; not physical-hardware validation",
        "cases": cases,
        "compared_rows": len(rows),
        "compared_writes": sum(len(row["write_deltas"]) for row in rows),
        "failing_rows": len(failing),
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    if len(rows) != 96 or report["compared_writes"] != 3060 or failing:
        raise SystemExit(f"FAIL: {len(failing)} timing rows differ")
    print(
        "PASS: 96 timing rows; all 3,060 DMA writes, finish, busy and Copper intervals match"
    )


if __name__ == "__main__":
    main()
