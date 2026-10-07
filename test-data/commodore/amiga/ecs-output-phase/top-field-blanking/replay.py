#!/usr/bin/env python3
"""Check corrected top-field output and retain the pre-fix negative controls."""

from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path


def main() -> None:
    base = Path(__file__).resolve().parent
    hashes = json.loads((base / "artifact-hashes.json").read_text())
    for name, expected in hashes.items():
        actual = hashlib.sha256((base / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"changed artifact: {name}")
    spec = importlib.util.spec_from_file_location(
        "compare_phase", base.parent / "tools/compare_phase.py"
    )
    if spec is None or spec.loader is None:
        raise ValueError("missing counter-domain comparator")
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    baseline = json.loads((base / "baseline-comparison.json").read_text())
    corrected = json.loads((base / "corrected-comparison.json").read_text())
    results = baseline["results"]
    if len(results) != 10 or len({r["case"] for r in results}) != 10:
        raise ValueError("expected ten distinct cases")
    failed_fields = 0
    corrected_fields = 0
    with tempfile.TemporaryDirectory(prefix="amiga-top-field-") as temporary:
        for expected in results:
            name = expected["case"]
            source = base.parent / "ecs-colour-blanking/aga" / name
            case = Path(temporary) / name
            capture = case / "reference/capture"
            capture.mkdir(parents=True)
            for path in (source / "capture").iterdir():
                if path.suffix == ".gz":
                    (capture / path.stem).write_bytes(
                        gzip.decompress(path.read_bytes())
                    )
                elif path.suffix == ".json":
                    shutil.copy2(path, capture / path.name)
            (case / "probe.adf").write_bytes(
                gzip.decompress((source / "probe.adf.gz").read_bytes())
            )
            shutil.copy2(source / "inputs.json", case / "inputs.json")
            shutil.copy2(source / "reference.log", case / "reference/run.log")
            shutil.copy2(base / name / "baseline.png", case / "baseline.png")
            actual = comparator.compare(case, "baseline.png", tag="AGA_ORIGIN")
            # Round-trip JSON to normalise tuples in pixel bounds.
            actual = json.loads(json.dumps(actual))
            if actual != expected:
                raise ValueError(f"comparison changed: {name}")
            if len(actual["fields"]) != 3:
                raise ValueError(f"missing fields: {name}")
            failed_fields += sum(
                field["mismatched_rgb_pixels"] != 0 for field in actual["fields"]
            )
            shutil.copy2(base / name / "corrected.png", case / "corrected.png")
            actual_fixed = json.loads(
                json.dumps(comparator.compare(case, "corrected.png", tag="AGA_ORIGIN"))
            )
            expected_fixed = next(r for r in corrected["results"] if r["case"] == name)
            if actual_fixed != expected_fixed or len(actual_fixed["fields"]) != 3:
                raise ValueError(f"corrected comparison changed: {name}")
            if any(field["mismatched_rgb_pixels"] for field in actual_fixed["fields"]):
                raise ValueError(f"corrected pixels differ: {name}")
            corrected_fields += len(actual_fixed["fields"])
    if failed_fields != 18:
        raise ValueError("expected eighteen failing fields and twelve exact controls")
    if corrected_fields != 30:
        raise ValueError("expected thirty corrected fields")
    print(
        "30 corrected fields exact; 18 pre-fix failures and 12 exact controls reproduced"
    )


if __name__ == "__main__":
    main()
