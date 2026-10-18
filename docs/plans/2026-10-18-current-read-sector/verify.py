#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Inspect this slice's captured evidence; never execute or qualify a runtime."""

from __future__ import annotations

import hashlib
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def verify(files: dict[str, bytes]) -> dict:
    proof = json.loads(files["verification.json"])
    for name in ["rom-1", "rom-2", "rom-wrong"]:
        record = json.loads(files[f"{name}/regression.json"])
        tool = record["tools"]["emu"]
        if (
            record["qualified"] is not False
            or tool["kind"] != "source_build"
            or tool["commit"] != proof["source_commit"]
            or tool["sha256"] != proof["executable_sha256"]
        ):
            raise ValueError("source/runtime scope or identity changed")
        if name == "rom-wrong":
            if record["regression_passed"] is not False or record.get("failure") != (
                "signature mismatch: expected [90, 92], observed [90, 91]"
            ):
                raise ValueError("wrong-result control did not reject")
            continue
        if record["regression_passed"] is not True:
            raise ValueError("ROM regression failed")
        for stage, count in [
            ("native_format", 519),
            ("firmware", 8),
            ("loading", 3),
            ("execution", 7),
        ]:
            result = record["stages"][stage]
            if result["status"] != "pass" or result["assertions"] != count:
                raise ValueError("missing bounded assertions")
        observed = record["stages"]["execution"]["observed"]
        if (
            observed["signature_before"] != [0, 0]
            or observed["signature_after"] != [90, 91]
            or observed["pc"] != 32820
            or len(observed["writes"]) != 4
        ):
            raise ValueError("missing executed state transition")
    if (
        proof["qualified"] is not False
        or proof["distributed_release_qualification"] is not False
    ):
        raise ValueError("false qualification")
    return {
        "qualified": False,
        "kind": "captured-evidence-inspection",
        "rom_passes": 2,
        "wrong_result_rejected": True,
        "source_commit": proof["source_commit"],
    }


def main() -> None:
    evidence = HERE / "evidence"
    wanted = json.loads((evidence / "hashes.json").read_text())
    files = {}
    with tarfile.open(evidence / "runs.tar.gz") as bundle:
        for member in bundle:
            if not member.isfile() or member.name not in wanted or member.name in files:
                raise ValueError("unexpected archive member")
            stream = bundle.extractfile(member)
            if stream is None:
                raise ValueError("missing archive member")
            data = stream.read()
            if hashlib.sha256(data).hexdigest() != wanted[member.name]:
                raise ValueError("artifact hash changed")
            files[member.name] = data
    if files.keys() != wanted.keys():
        raise ValueError("missing artifact")
    print(json.dumps(verify(files), indent=2))


if __name__ == "__main__":
    main()
