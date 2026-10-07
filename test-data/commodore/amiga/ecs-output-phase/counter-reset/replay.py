#!/usr/bin/env python3
"""Replay the reset-boundary fault and correction against fixed reference fields."""

from __future__ import annotations

import gzip
import hashlib
import importlib.util
import json
import re
import shutil
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parent


def restore_case(source: Path, case: Path, *, nested: bool) -> None:
    capture = case / "reference/capture"
    capture.mkdir(parents=True)
    reference = source / "reference" if nested else source
    fields = list((reference / "capture").glob("*.bgra.gz"))
    if len(fields) != 3:
        raise ValueError("three reference fields are required")
    for packed in fields:
        (capture / packed.stem).write_bytes(gzip.decompress(packed.read_bytes()))
    for metadata in (reference / "capture").glob("*.json"):
        shutil.copy2(metadata, capture / metadata.name)
    (case / "probe.adf").write_bytes(
        gzip.decompress((source / "probe.adf.gz").read_bytes())
    )
    shutil.copy2(source / "inputs.json", case / "inputs.json")
    log = (
        gzip.decompress((reference / "run.log.gz").read_bytes())
        if nested
        else (source / "reference.log").read_bytes()
    )
    (case / "reference/run.log").write_bytes(log)
    if nested:
        resets = set()
        for line in log.decode().splitlines():
            if "AGA_PHASE " not in line:
                continue
            values = {
                k: int(v)
                for k, v in re.findall(r"\b(cnt|next|half|x|line)=(-?\d+)", line)
            }
            if values.get("next") == 2:
                if (values["cnt"], values["half"], values["x"]) != (455, 1, 1456):
                    raise ValueError("reset-counter trace changed")
                resets.add(values["line"])
        if len(resets) < 3:
            raise ValueError("missing positive counter-reset observations")


def verify(base: Path = BASE) -> None:
    hashes = json.loads((base / "artifact-hashes.json").read_text())
    for name, expected in hashes.items():
        if hashlib.sha256((base / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f"artifact hash mismatch: {name}")
    spec = importlib.util.spec_from_file_location(
        "counter_comparison", BASE.parent / "tools/compare_phase.py"
    )
    if spec is None or spec.loader is None:
        raise ValueError("missing registered comparator")
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    failed = 0
    exact = 0
    for report, image, expected_count in [
        ("baseline", "baseline", 20),
        ("corrected", "corrected", 20),
        ("controls", "reset-control", 10),
    ]:
        records = json.loads((base / f"{report}-comparison.json").read_text())[
            "results"
        ]
        if (
            len(records) != expected_count
            or len({r["case"] for r in records}) != expected_count
        ):
            raise ValueError("missing or duplicate cases")
        with tempfile.TemporaryDirectory(prefix="blank-reset-replay-") as directory:
            for expected in records:
                name = expected["case"]
                controls = report == "controls"
                artifacts = base / ("controls" if controls else "guests") / name
                source = (
                    BASE.parent / "ecs-colour-blanking/aga" / name
                    if controls
                    else artifacts
                )
                case = Path(directory) / name
                restore_case(source, case, nested=not controls)
                shutil.copy2(artifacts / f"{image}.png", case / "native.png")
                actual = json.loads(
                    json.dumps(comparator.compare(case, "native.png", tag="AGA_ORIGIN"))
                )
                if actual != expected or len(actual["fields"]) != 3:
                    raise ValueError(f"comparison changed: {report}/{name}")
                if report == "baseline":
                    failed += sum(
                        f["mismatched_rgb_pixels"] != 0 for f in actual["fields"]
                    )
                else:
                    if any(f["mismatched_rgb_pixels"] for f in actual["fields"]):
                        raise ValueError(f"corrected output differs: {name}")
                    exact += 3
    if (failed, exact) != (24, 90):
        raise ValueError(
            f"unexpected coverage: {failed} failing / {exact} exact fields"
        )
    print(
        "90 corrected fields exact; 24 old failures and 36 exact baseline controls reproduced"
    )


if __name__ == "__main__":
    verify()
