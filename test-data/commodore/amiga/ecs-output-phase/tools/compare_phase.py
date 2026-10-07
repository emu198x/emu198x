#!/usr/bin/env python3
"""Compare ECS diagnostic pixels using independently traced counter origins.

This is a counter-domain diagnostic, not a replacement for the admitted video
gate or a physical-beam calibration. No image content is used for alignment.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
from pathlib import Path

from PIL import Image, ImageChops


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def traced_origin(
    log: str, metadata_origin: int, *, tag: str = "ECS_ORIGIN"
) -> tuple[int, int]:
    """Require complete, consistent evidence for all 200 active lines/field."""
    rows: dict[tuple[int, int], tuple[int, int]] = {}
    for line in log.splitlines():
        if tag + " " not in line:
            continue
        values = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", line)}
        if not all(
            k in values for k in ("guest", "v", "counter", "x", "shift", "lol", "ecs")
        ):
            raise ValueError("incomplete origin trace row")
        if values["guest"] not in (9, 10, 11) or not 44 <= values["v"] < 244:
            continue
        if values["lol"] != 0 or values["ecs"] not in (0, 1):
            raise ValueError("this diagnostic requires settled PAL line phase")
        origin = values["counter"] * 4 - values["x"]
        shift = values["shift"]
        if origin != metadata_origin - shift:
            raise ValueError("counter origin disagrees with recorded output padding")
        key = (values["guest"], values["v"])
        if key in rows:
            raise ValueError("duplicate active origin observation")
        rows[key] = (origin, shift)
    expected = {(field, line) for field in (9, 10, 11) for line in range(44, 244)}
    if rows.keys() != expected:
        raise ValueError("origin evidence must cover all 600 active line observations")
    levels = set(rows.values())
    if len(levels) != 1:
        raise ValueError("varying line origins need a different comparison")
    return levels.pop()


def compare(
    case: Path, native_name: str, *, tag: str = "ECS_ORIGIN"
) -> dict[str, object]:
    # Reuse the existing identity, complete-field, metadata and raster checks.
    path = Path(__file__).resolve().parents[2] / "wide-sprite-dma/tools/compare.py"
    spec = importlib.util.spec_from_file_location("full_raster", path)
    if spec is None or spec.loader is None:
        raise ValueError("missing whole-raster comparator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    baseline = module.compare(case, native_name)
    fields = sorted((case / "reference/capture").glob("*.bgra"))
    metadata = json.loads(fields[0].with_suffix(".json").read_text())
    reported_origin = metadata["framebuffer"]["inbuffer_xoffset"]
    trace = case / "reference/run.log"
    origin, padding = traced_origin(trace.read_text(), reported_origin, tag=tag)
    if not 0 <= padding <= 8:
        raise ValueError("output padding outside this diagnostic's bounded raster")
    native = Image.open(case / native_name).convert("RGB")
    width = 1512 - padding
    observed = native.crop((16, 2, 16 + width, 576))
    results = []
    for raw in fields:
        reference = Image.frombytes(
            "RGBA", (1512, 576), raw.read_bytes(), "raw", "BGRA"
        ).convert("RGB")
        difference = ImageChops.difference(
            observed, reference.crop((padding, 0, 1512, 574))
        )
        rgb = difference.tobytes()
        count = sum(
            any(pixel) for pixel in zip(rgb[::3], rgb[1::3], rgb[2::3], strict=True)
        )
        results.append(
            {
                "raw_sha256": sha256(raw),
                "mismatched_rgb_pixels": count,
                "compared_rgb_pixels": width * 574,
                "mismatch_bounds": difference.getbbox(),
            }
        )
    return {
        "case": case.name,
        "baseline": baseline,
        "trace_sha256": sha256(trace),
        "reference_counter_origin": origin,
        "reported_origin": reported_origin,
        "output_padding_samples": padding,
        "active_origin_observations": 600,
        "common_raster": {
            "native_origin": [16, 2],
            "reference_origin": [padding, 0],
            "width": width,
            "height": 574,
        },
        "fields": results,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("guests", type=Path)
    parser.add_argument("--native", default="after-doubled.png")
    parser.add_argument(
        "--trace-tag", choices=("ECS_ORIGIN", "AGA_ORIGIN"), default="ECS_ORIGIN"
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    cases = json.loads((args.guests / "diagnostics.json").read_text())["cases"]
    if not cases or len({c["case"] for c in cases}) != len(cases):
        parser.error("manifest must contain distinct nonempty cases")
    results = []
    for record in cases:
        case = args.guests / record["case"]
        if sha256(case / "probe.adf") != record["adf_sha256"]:
            raise ValueError("guest differs from manifest")
        if sha256(case / args.native) != record["native_sha256"]:
            raise ValueError("native capture differs from manifest")
        results.append(compare(case, args.native, tag=args.trace_tag))
    failed = sum(f["mismatched_rgb_pixels"] != 0 for r in results for f in r["fields"])
    report = {
        "scope": "source-traced counter-domain comparison; no physical-beam claim",
        "compared_fields": len(results) * 3,
        "failed_fields": failed,
        "results": results,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(results)} cases, {len(results) * 3} fields, {failed} failures")
    if failed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
