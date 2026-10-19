#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Reuse the preceding fixture runner with this finite matrix and register hosts."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tarfile
from pathlib import Path

from cases import input_text, matrix, raw_track

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PRIOR = HERE.parent / "2026-10-19-read-track-field-boundaries"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    spec = importlib.util.spec_from_file_location(
        "preceding_fields_runner", PRIOR / "run.py"
    )
    if spec is None or spec.loader is None:
        raise ValueError("missing preceding runner")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    helpers = {
        str(p.relative_to(REPO)): sha(p) for p in (PRIOR / "run.py", PRIOR / "cases.py")
    }
    runner.HERE = HERE
    runner.matrix = matrix
    runner.input_text = input_text
    runner.raw_track = raw_track
    # Runner, compiler, firmware isolation and host limits remain the previous mechanism.
    runner.main()
    out = HERE / "runs"
    path = out / "campaign.json"
    record = json.loads(path.read_text())
    record["kind"] = "read-track-opcode-and-missing-entry-contrasts"
    record["configuration"]["scope"] = (
        "isolated/direct register hosts; no ROM loading qualification; see configuration-scope.json"
    )
    record["helper_sources"] = helpers
    if record["status"] == "captured":
        # Preserve imported implementation identities in the actual source archive too.
        temporary = out / "source-expanded.tar.gz"
        with (
            tarfile.open(out / "capture-source.tar.gz") as old,
            tarfile.open(temporary, "w:gz") as new,
        ):
            for member in old.getmembers():
                new.addfile(member, old.extractfile(member))
            for name, want in helpers.items():
                if sha(REPO / name) != want:
                    raise ValueError("imported helper changed during capture")
                new.add(REPO / name, arcname=name, recursive=False)
        temporary.replace(out / "capture-source.tar.gz")
        for row in record["observations"]:
            if row["status"] == "not_applicable":
                row["reason"] = (
                    "missing sector-image track entry has no faithful raw-track equivalent in this fixture; no magnetic-media equivalence claimed"
                )
        # The engine's complete observation values are retained in campaign.json.
        # Source/asset hashes are checked again by the independent offline auditor.
        record["result_framing"] = (
            "one invalid-command byte after opcode rejection, otherwise all seven accepted-command result bytes; unsent parameters are never submitted"
        )
    path.write_text(json.dumps(record, indent=2) + "\n")
    hashes = {
        p.name: sha(p) for p in out.iterdir() if p.is_file() and p.name != "hashes.json"
    }
    (out / "hashes.json").write_text(json.dumps(hashes, indent=2) + "\n")


if __name__ == "__main__":
    main()
