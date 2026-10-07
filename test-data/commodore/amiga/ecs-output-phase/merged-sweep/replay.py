#!/usr/bin/env python3.13
"""Replay all 384 registered comparisons, including a pixel-error control."""

from __future__ import annotations

import gzip
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mismatch_count(actual: Image.Image, reference: Image.Image) -> int:
    difference = ImageChops.difference(actual, reference)
    return sum(pixel != (0, 0, 0) for pixel in difference.get_flattened_data())


def replay(base: Path) -> dict[str, int]:
    report = json.loads((base / "comparison.json").read_text())
    results = report["results"]
    if len(results) != 128 or len({r["archive_case"] for r in results}) != 128:
        raise ValueError("expected all 128 distinct guests")
    if report["compared_fields"] != 384:
        raise ValueError("expected 384 reference-field comparisons")
    registered = json.loads((base / "producers.json").read_text())
    fields_checked = 0
    failed_fields = 0
    samples_checked = 0
    negative_controls = 0
    for result in results:
        case = base / result["archive_case"]
        original = result["path"]
        adf = gzip.decompress((case / "probe.adf.gz").read_bytes())
        if (
            digest(adf) != result["adf_sha256"]
            or digest(adf) != registered[original + "/probe.adf"]
        ):
            raise ValueError(f"{case}: guest hash differs")
        for name in ["merged56.png", "merged56.log", "inputs.json"]:
            if digest((case / name).read_bytes()) != result[name + "_sha256"]:
                raise ValueError(f"{case}: altered {name}")
        identity = json.loads((case / "inputs.json").read_text())["identity"]["serial"]
        observations = json.loads((case / "merged56.log").read_text())["observations"]
        ready = bytes(
            next(row for row in observations if row["kind"] == "memory_read")["bytes"]
        )
        if ready[:4] != b"SPHX" or int.from_bytes(ready[8:12], "big") < 9:
            raise ValueError(f"{case}: guest not ready")
        if ready[64:].split(b"\0", 1)[0].decode() != identity:
            raise ValueError(f"{case}: native guest identity differs")
        native = Image.open(case / "merged56.png").convert("RGB")
        if native.size != (1536, 576):
            raise ValueError(f"{case}: wrong native extent")
        actual = native.crop((16, 2, 1524, 576))
        raw_files = sorted((case / "reference").glob("*.bgra.gz"))
        if len(raw_files) != 3 or len(result["fields"]) != 3:
            raise ValueError(f"{case}: expected three reference fields")
        counters = []
        for raw, retained in zip(raw_files, result["fields"], strict=True):
            raw_name = raw.name.removesuffix(".gz")
            meta_path = raw.with_name(raw_name.removesuffix(".bgra") + ".json")
            meta_bytes = meta_path.read_bytes()
            if (
                digest(meta_bytes)
                != registered[original + "/reference/capture/" + meta_path.name]
            ):
                raise ValueError(f"{case}: reference metadata differs")
            meta = json.loads(meta_bytes)
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
            if any(meta["framebuffer"].get(k) != v for k, v in expected.items()):
                raise ValueError(f"{case}: reference raster metadata differs")
            if (
                meta["ready"]["identity"] != identity
                or meta["ready"]["case_number"] != 1
            ):
                raise ValueError(f"{case}: reference guest identity differs")
            counters.append(meta["guest_field_counter"])
            data = gzip.decompress(raw.read_bytes())
            if len(data) != 1512 * 576 * 4:
                raise ValueError(f"{case}: truncated reference")
            if (
                digest(data) != retained["raw_sha256"]
                or digest(data)
                != registered[original + "/reference/capture/" + raw_name]
            ):
                raise ValueError(f"{case}: reference hash differs")
            reference = (
                Image.frombytes("RGBA", (1512, 576), data, "raw", "BGRA")
                .convert("RGB")
                .crop((4, 0, 1512, 574))
            )
            mismatches = mismatch_count(actual, reference)
            if mismatches != retained["mismatched_rgb_pixels"]:
                raise ValueError(f"{case}: result differs: {mismatches}")
            fields_checked += 1
            failed_fields += mismatches != 0
            samples_checked += 1508 * 574
            if mismatches == 0 and negative_controls == 0:
                altered = actual.copy()
                red, green, blue = altered.getpixel((0, 0))
                altered.putpixel((0, 0), (red ^ 1, green, blue))
                if mismatch_count(altered, reference) != 1:
                    raise ValueError(
                        "comparison failed to detect one changed RGB sample"
                    )
                negative_controls += 1
        if counters != [9, 10, 11]:
            raise ValueError(f"{case}: reference fields are not adjacent: {counters}")
    if fields_checked != 384 or samples_checked != 332387328 or negative_controls != 1:
        raise ValueError("incomplete comparison or missing negative control")
    if failed_fields != report["failed_fields"]:
        raise ValueError("failure total differs from report")
    summary = {
        "guests": 128,
        "compared_fields": fields_checked,
        "failed_fields": failed_fields,
        "compared_rgb_samples": samples_checked,
        "negative_controls": negative_controls,
    }
    print(json.dumps(summary, indent=2))
    if failed_fields:
        raise ValueError(f"{failed_fields} reference comparisons failed")
    return summary


if __name__ == "__main__":
    replay(Path(__file__).resolve().parent)
