#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Compare a READY-validated Copper WAIT case over the full common PAL raster."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

from PIL import Image


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("guests", type=Path)
    parser.add_argument(
        "--case", choices=("bfd-ignore", "bfd-wait"), default="bfd-ignore"
    )
    parser.add_argument("--native", default="after.png")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads((args.guests / "diagnostics.json").read_text())
    cases = manifest["cases"]
    if len(cases) != 2 or {case["case"] for case in cases} != {
        "bfd-ignore",
        "bfd-wait",
    }:
        parser.error("manifest must contain exactly the paired WAIT cases")
    for record in cases:
        directory = args.guests / record["case"]
        for filename, expected in (
            ("probe.adf", record["adf_sha256"]),
            ("src/probe.S", record["source_sha256"]),
        ):
            if (
                hashlib.sha256((directory / filename).read_bytes()).hexdigest()
                != expected
            ):
                raise ValueError(
                    f"{directory / filename}: differs from builder manifest"
                )
    case = args.guests / args.case
    observations = json.loads(
        (case / Path(args.native).with_suffix(".log")).read_text()
    )["observations"]
    record = next(
        item
        for item in observations
        if item["kind"] == "memory_read" and item["addr"] == 0x2FF00
    )
    ready = bytes(record["bytes"])
    identity = json.loads((case / "inputs.json").read_text())["identity"]["serial"]
    if (
        len(ready) < 128
        or ready[:4] != b"SPHX"
        or ready[4:8] != b"\0\1\0\1"
        or int.from_bytes(ready[8:12], "big") < 9
        or ready[64:].split(b"\0", 1)[0].decode() != identity
    ):
        raise ValueError(f"{case}: native guest READY identity/counter is invalid")
    native = Image.open(case / args.native).convert("RGB")
    if (
        sum(
            red > 200 and green < 20 and blue < 20
            for red, green, blue in native.get_flattened_data()
        )
        < 1000
    ):
        raise ValueError(f"{case}: red timing markers are absent")
    common = Path(__file__).resolve().parents[2] / "wide-sprite-dma/tools/compare.py"
    spec = importlib.util.spec_from_file_location("common_raster_compare", common)
    if spec is None or spec.loader is None:
        raise ValueError("common raster comparator could not be loaded")
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    result = comparator.compare(case, args.native)
    failures = sum(field["mismatched_rgb_pixels"] != 0 for field in result["fields"])
    report = {
        "boundary": "software-reference diagnostic; not silicon validation",
        "case": args.case,
        "compared_fields": 3,
        "failed_fields": failures,
        "timing_validation": "raster comparison does not validate DMA transfers or busy boundaries; check the independent event trace",
        "result": result,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{args.case}: 3 full-raster comparisons, {failures} failed fields")
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
