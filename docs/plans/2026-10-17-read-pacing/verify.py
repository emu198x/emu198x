#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Check captured safe evidence offline; this never executes an emulator."""

from __future__ import annotations

import hashlib
import json
import tarfile
from pathlib import Path

from reference import check

HERE = Path(__file__).resolve().parent


def verify(files: dict[str, bytes]) -> dict:
    reference = json.loads(files["reference/result.json"])
    if reference["qualified"] is not False or reference["origin"] != "fresh_execution":
        raise ValueError("reference scope changed")
    expected = {
        (repeat, op, pause)
        for repeat in [1, 2]
        for op in [70, 76, 66]
        for pause in [0, 1000]
    }
    if (
        len(reference["cases"]) != 12
        or {(c["repeat"], c["opcode"], c["pause_us"]) for c in reference["cases"]}
        != expected
    ):
        raise ValueError("missing reference case")
    for case in reference["cases"]:
        if case["exit"] != 0 or case["media_unchanged"] is not True:
            raise ValueError("reference process/media failure")
        check(case["observed"], case["pause_us"])
    source = "eb26c4b59464d46f111a66d0e060a813bbd81b2a"
    binary = "30548d7940f9066a43abb4e3486bfdfb03a60e5ca6fefd80094dda8e63650b3e"
    for name in ["rom-1", "rom-2", "rom-wrong"]:
        record = json.loads(files[f"{name}/regression.json"])
        if (
            record["qualified"] is not False
            or record["tools"]["emu"]["kind"] != "source_build"
            or record["tools"]["emu"]["commit"] != source
            or record["tools"]["emu"]["sha256"] != binary
        ):
            raise ValueError("source build identity/scope changed")
        if name == "rom-wrong":
            if (
                record["regression_passed"]
                or record["failure"]
                != "signature mismatch: expected [90, 92], observed [90, 91]"
            ):
                raise ValueError("negative execution control did not fail")
        else:
            if record["regression_passed"] is not True:
                raise ValueError("ROM regression failed")
            for stage in ["native_format", "firmware", "loading", "execution"]:
                if (
                    record["stages"][stage]["status"] != "pass"
                    or record["stages"][stage]["assertions"] <= 0
                ):
                    raise ValueError("missing runtime assertions")
            observed = record["stages"]["execution"]["observed"]
            if (
                observed["signature_before"] != [0, 0]
                or observed["signature_after"] != [90, 91]
                or len(observed["writes"]) != 4
            ):
                raise ValueError("missing executed state transition")
    first = json.loads(files["startup-failure/regression.json"])
    if (
        first["regression_passed"]
        or first["failure"] != "ROM loader did not load the assembled payload"
    ):
        raise ValueError("startup failure lost")
    return {
        "schema_version": 1,
        "qualified": False,
        "kind": "captured-evidence-inspection",
        "reference_observations": 12,
        "fresh_rom_passes": 2,
        "wrong_signature_rejected": True,
        "initial_startup_failure_retained": True,
        "exact_timing_conformance": False,
    }


def main() -> None:
    evidence = HERE / "evidence"
    expected = json.loads((evidence / "hashes.json").read_text())
    files = {}
    with tarfile.open(evidence / "runs.tar.gz") as bundle:
        for member in bundle:
            if (
                not member.isfile()
                or member.name not in expected
                or member.name in files
            ):
                raise ValueError("unexpected archive member")
            stream = bundle.extractfile(member)
            if stream is None:
                raise ValueError("missing archive member")
            data = stream.read()
            if hashlib.sha256(data).hexdigest() != expected[member.name]:
                raise ValueError("artifact hash differs")
            files[member.name] = data
    if files.keys() != expected.keys():
        raise ValueError("missing artifact")
    print(json.dumps(verify(files), indent=2))


if __name__ == "__main__":
    main()
