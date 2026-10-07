#!/usr/bin/env python3
"""Replay counter-qualified XOR fields and their pre-fix negative controls."""

from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path


def main() -> None:
    archive = Path(__file__).resolve().parent
    manifest = json.loads((archive / "validation.json").read_text())
    for name, expected in manifest["artifact_sha256"].items():
        if hashlib.sha256((archive / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f"archive hash mismatch: {name}")
    comparator = archive.parent / "tools/compare_phase.py"
    spec = importlib.util.spec_from_file_location("phase_comparator", comparator)
    if spec is None or spec.loader is None:
        raise ValueError("missing phase comparator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    results = []
    with tempfile.TemporaryDirectory(prefix="amiga-xor-replay-") as temporary:
        for name in ("constant", "varying", "reset"):
            case = Path(temporary) / name
            for source in (archive / name).rglob("*"):
                if not source.is_file():
                    continue
                target = case / source.relative_to(archive / name)
                target.parent.mkdir(parents=True, exist_ok=True)
                if source.suffix == ".gz":
                    target.with_suffix("").write_bytes(
                        gzip.decompress(source.read_bytes())
                    )
                else:
                    shutil.copy2(source, target)
            for image, count in [("before54.png", 128), ("after54.png", 0)]:
                result = module.compare(case, image, tag="AGA_ORIGIN")
                if len(result["fields"]) != 3 or any(
                    f["mismatched_rgb_pixels"] != count for f in result["fields"]
                ):
                    raise ValueError(
                        f"{name}/{image}: unexpected full-field comparison"
                    )
                results.append(result)
    print(
        json.dumps(
            {"exact_fields": 9, "negative_fields": 9, "results": results}, indent=2
        )
    )


if __name__ == "__main__":
    main()
