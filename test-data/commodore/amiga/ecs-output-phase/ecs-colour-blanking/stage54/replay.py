#!/usr/bin/env python3
"""Replay archived ECS blanking comparisons, including the pre-fix control."""

from __future__ import annotations

import gzip
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path


def main() -> None:
    archive = Path(__file__).resolve().parent
    comparator = archive.parents[1] / "tools/compare_phase.py"
    spec = importlib.util.spec_from_file_location("phase_comparator", comparator)
    if spec is None or spec.loader is None:
        raise ValueError("missing phase comparator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    identities = json.loads((archive / "identities.json").read_text())
    names = {row["case"] for row in identities}
    expected = {
        "blanken-path",
        "color-moves",
        "ecsena-gate",
        "extblken-gate",
        "fixed-control",
        "programmed-central",
        "programmed-equal",
        "programmed-wrap",
    }
    if names != expected or len(identities) != 8:
        raise ValueError("expected all eight ECS controls exactly once")
    results = []
    negative = None
    with tempfile.TemporaryDirectory(prefix="ecs-blanking-stage54-") as temporary:
        for name in sorted(names):
            case = Path(temporary) / name
            source = archive.parent / "ecs" / name
            capture = case / "reference/capture"
            capture.mkdir(parents=True)
            for raw in sorted((source / "capture").glob("*.bgra.gz")):
                (capture / raw.stem).write_bytes(gzip.decompress(raw.read_bytes()))
            for metadata in (source / "capture").glob("*.json"):
                shutil.copy2(metadata, capture / metadata.name)
            shutil.copy2(source / "reference.log", case / "reference/run.log")
            shutil.copy2(source / "inputs.json", case / "inputs.json")
            (case / "probe.adf").write_bytes(
                gzip.decompress((source / "probe.adf.gz").read_bytes())
            )
            shutil.copy2(
                archive / name / "stage54-doubled.png", case / "stage54-doubled.png"
            )
            result = module.compare(case, "stage54-doubled.png")
            if len(result["fields"]) != 3 or any(
                field["mismatched_rgb_pixels"] for field in result["fields"]
            ):
                raise ValueError(f"{name}: corrected full-field comparison failed")
            results.append(result)
            if name == "programmed-central":
                shutil.copy2(source / "before-doubled.png", case / "before.png")
                negative = module.compare(case, "before.png")
                if [field["mismatched_rgb_pixels"] for field in negative["fields"]] != [
                    32032
                ] * 3:
                    raise ValueError(
                        "pre-fix negative control no longer detects the missing delay"
                    )
    if negative is None:
        raise ValueError("negative control was not executed")
    report = {
        "exact_fields": len(results) * 3,
        "results": results,
        "negative_control": negative,
    }
    (archive / "archive-replay.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        "24 exact fields; pre-fix negative control differs at 32,032 pixels in each of three fields."
    )


if __name__ == "__main__":
    main()
