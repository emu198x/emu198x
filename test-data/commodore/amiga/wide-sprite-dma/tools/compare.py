#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Compare every RGB sample in the source-defined common PAL native raster."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compare(case: Path, native_name: str) -> dict[str, object]:
    native_path = case / native_name
    native = Image.open(native_path).convert("RGB")
    if native.size != (1536, 576):
        raise ValueError(f"{native_path}: expected full 1536x576 native raster")
    fields = sorted((case / "reference/capture").glob("*.bgra"))
    if len(fields) != 3:
        raise ValueError(f"{case}: expected three complete reference fields")
    records = []
    counters = []
    for raw in fields:
        metadata_path = raw.with_suffix(".json")
        metadata = json.loads(metadata_path.read_text())
        fb = metadata["framebuffer"]
        expected = {
            "width": 1512,
            "height": 576,
            "packed_output_stride_bytes": 6048,
            "pixel_format": "BGRA8888",
            "inbuffer_xoffset": 368,
            "inbuffer_yoffset": 52,
            "host_resolution": 2,
            "complete": True,
        }
        if any(fb.get(key) != value for key, value in expected.items()):
            raise ValueError(f"{raw}: incompatible raster metadata: {fb}")
        if metadata["ready"]["case_number"] != 1:
            raise ValueError(f"{raw}: wrong guest ready record")
        identity = json.loads((case / "inputs.json").read_text())["identity"]["serial"]
        if metadata["ready"]["identity"] != identity:
            raise ValueError(f"{raw}: guest identity mismatch")
        counters.append(metadata["guest_field_counter"])
        data = raw.read_bytes()
        if len(data) != 1512 * 576 * 4:
            raise ValueError(f"{raw}: incomplete packed framebuffer")
        reference = Image.frombytes("RGBA", (1512, 576), data, "raw", "BGRA").convert(
            "RGB"
        )
        # Buffer offsets are actual captured beam origins, not fitted offsets.
        observed = native.crop((16, 2, 1528, 576))
        expected_image = reference.crop((0, 0, 1512, 574))
        difference = ImageChops.difference(observed, expected_image)
        rgb = difference.tobytes()
        mismatches = sum(
            any(pixel) for pixel in zip(rgb[::3], rgb[1::3], rgb[2::3], strict=True)
        )
        records.append(
            {
                "guest_field": counters[-1],
                "compared_rgb_pixels": 1512 * 574,
                "mismatched_rgb_pixels": mismatches,
                "mismatch_bounds": difference.getbbox(),
                "raw_sha256": sha256(raw),
                "metadata_sha256": sha256(metadata_path),
            }
        )
    if counters != [9, 10, 11]:
        raise ValueError(
            f"{case}: expected adjacent guest fields 9, 10, 11; got {counters}"
        )
    return {
        "case": case.name,
        "native_sha256": sha256(native_path),
        "adf_sha256": sha256(case / "probe.adf"),
        "fields": records,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("guests", type=Path)
    parser.add_argument("--native", default="after.png")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads((args.guests / "diagnostics.json").read_text())
    cases = manifest["cases"]
    if not cases or len({record["case"] for record in cases}) != len(cases):
        parser.error("manifest must contain distinct nonempty cases")
    for record in cases:
        if (
            "adf_sha256" in record
            and sha256(args.guests / record["case"] / "probe.adf")
            != record["adf_sha256"]
        ):
            raise ValueError(
                f"{record['case']}: guest ADF differs from the builder manifest"
            )
    results = [compare(args.guests / record["case"], args.native) for record in cases]
    failures = sum(
        field["mismatched_rgb_pixels"] != 0
        for case in results
        for field in case["fields"]
    )
    report = {
        "status": "software diagnostic; not physical hardware calibration",
        "common_raster": {
            "width": 1512,
            "height": 574,
            "native_origin": [16, 2],
            "reference_origin": [0, 0],
        },
        "nonoverlap": {
            "native": "top 2 rows; remaining rows' left 16 and right 8 columns",
            "reference": "bottom 2 rows",
            "reason": "different producer origins and raster extents; no interior samples excluded",
        },
        "compared_fields": len(results) * 3,
        "failed_fields": failures,
        "results": results,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"{len(results)} cases, {len(results) * 3} full-raster comparisons, {failures} failed fields"
    )
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
