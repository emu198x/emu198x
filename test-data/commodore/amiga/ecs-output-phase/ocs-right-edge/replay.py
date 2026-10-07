#!/usr/bin/env python3
"""Verify the retained OCS edge fault and its positive producer trace."""

from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import re
import shutil
import tempfile
from pathlib import Path


def verify(root: Path) -> None:
    for name, expected in json.loads((root / "artifact-hashes.json").read_text()).items():
        actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"artifact hash mismatch: {name}")
    log = gzip.decompress((root / "reference.log.gz").read_bytes()).decode()
    rows = {}
    for line in log.splitlines():
        if "OCS_EDGE " not in line:
            continue
        values = dict(re.findall(r"(\w+)=([0-9a-f]+)", line.split("OCS_EDGE ", 1)[1]))
        key = tuple(int(values[k]) for k in ("guest", "v", "counter"))
        if key[0] in (9, 10, 11) and 44 <= key[1] < 244:
            if key in rows:
                raise ValueError("duplicate active trace observation")
            rows[key] = values
    expected_rows = {
        (guest, line, counter)
        for guest in (9, 10, 11)
        for line in range(44, 244)
        for counter in range(12, 17)
    }
    if rows.keys() != expected_rows:
        raise ValueError("incomplete active trace coverage")
    for (_, _, counter), values in rows.items():
        blank = counter >= 15  # Each entry observes the preceding tick's writes.
        if (
            int(values["x"]) != 1500 + (counter - 12) * 4
            or int(values["hb"]) != blank
            or int(values["blank"]) != blank
            or int(values["vb"]) != 0
            or any(
                bool(int(values[f"prev{i}"], 16) & 0xFFFFFF) == blank
                for i in range(4)
            )
        ):
            raise ValueError("blank transition or actual buffer writes disagree")

    spec = importlib.util.spec_from_file_location(
        "counter_comparison", root.parent / "tools/compare_phase.py"
    )
    if spec is None or spec.loader is None:
        raise ValueError("missing registered counter comparator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    cases = [
        ("ocs", "color-moves", root, "ocs-edge-baseline-doubled.png", 2288),
        ("ocs", "color-moves", root, "ocs-edge-after-doubled.png", 0),
        ("ecs", "programmed-central", root / "ecs", "ocs-edge-control-doubled.png", 0),
        ("aga", "programmed-central", root / "aga", "ocs-edge-control.png", 0),
    ]
    for profile, name, artifact, image, mismatches in cases:
        retained = root.parent / "ecs-colour-blanking" / profile / name
        with tempfile.TemporaryDirectory(prefix="ocs-right-edge-") as directory:
            case = Path(directory) / name
            capture = case / "reference/capture"
            capture.mkdir(parents=True)
            for packed in (retained / "capture").glob("*.bgra.gz"):
                (capture / packed.stem).write_bytes(gzip.decompress(packed.read_bytes()))
            for metadata in (retained / "capture").glob("*.json"):
                shutil.copy2(metadata, capture / metadata.name)
            (case / "probe.adf").write_bytes(
                gzip.decompress((retained / "probe.adf.gz").read_bytes())
            )
            shutil.copy2(retained / "inputs.json", case / "inputs.json")
            shutil.copy2(artifact / image, case / "native.png")
            (case / "reference/run.log").write_text(
                log if profile == "ocs" else (retained / "reference.log").read_text()
            )
            result = module.compare(
                case, "native.png", tag="AGA_ORIGIN" if profile == "aga" else "ECS_ORIGIN"
            )
            if len(result["fields"]) != 3 or any(
                field["mismatched_rgb_pixels"] != mismatches
                or (
                    tuple(field["mismatch_bounds"]) != (1504, 0, 1508, 572)
                    if mismatches
                    else field["mismatch_bounds"] is not None
                )
                for field in result["fields"]
            ):
                raise ValueError(f"unexpected whole-raster comparison: {profile}/{image}")
    print("Verified 3,000 trace observations, 3 old failures and 9 exact corrected fields")



if __name__ == "__main__":
    verify(Path(__file__).resolve().parent)
