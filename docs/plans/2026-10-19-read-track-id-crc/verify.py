#!/usr/bin/env python3
"""Audit captured #1849 evidence; this does not execute an emulator."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = "3bab241b99600ebfad0945e15fc073358e4a41ef"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unpack(path: Path) -> dict[str, bytes]:
    with tarfile.open(path) as archive:
        result = {}
        for member in archive.getmembers():
            p = Path(member.name)
            assert member.isfile() and not p.is_absolute() and ".." not in p.parts
            assert member.name not in result
            stream = archive.extractfile(member)
            assert stream is not None
            result[member.name] = stream.read()
        assert result
        return result


def audit_runtime(files: dict[str, bytes]) -> None:
    checks = json.loads(files["checks.json"])
    assert [c["exit"] for c in checks] == [0, 0, 1]
    assert [c["name"] for c in checks] == [
        "positive-1",
        "positive-2",
        "wrong-signature",
    ]
    for name in ["positive-1", "positive-2", "wrong-signature"]:
        r = json.loads(files[name + "/regression.json"])
        positive = name != "wrong-signature"
        assert r["qualified"] is False and r["regression_passed"] is positive
        assert r["fixture"]["origin"] == "captured_reference_replay"
        assert (
            r["evidence_class"] == "fresh_execution" and r["delivery"] == "source_build"
        )
        assert r["repositories"]["emu"]["commit"] == SOURCE
        assert r["repositories"]["emu"]["tracked_status"] == ""
        assert (
            r["tools"]["emu"]["sha256"]
            == "f414a1e0cb343e8ab3aa33d73c3141a003390ca96e5981b75d9bba4e0d0eeab3"
        )
        assert (
            r["configuration"]["id"]
            == "spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1"
        )
        for artifact in r["artifacts"]:
            data = files[name + "/" + artifact["path"]]
            assert len(data) == artifact["size"] and sha(data) == artifact["sha256"]
        for stage, count in [("native_format", 519), ("firmware", 8), ("loading", 3)]:
            assert r["stages"][stage]["status"] == "pass"
            assert r["stages"][stage]["assertions"] == count
        assert all(
            r["stages"][s]["status"] == "not_run" for s in ["assembly", "mastering"]
        )
        execution = r["stages"]["execution"]
        if positive:
            assert execution["status"] == "pass" and execution["assertions"] == 7
            observed = execution["observed"]
            assert observed["signature_before"] == [0, 0]
            assert observed["signature_after"] == [90, 91]
            assert (
                observed["screen_sha256"]
                == "d2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2"
            )
            assert observed["writes"][-2:] == [
                {"pc": 32816, "addr": 28672, "value": 90, "size_bytes": 1},
                {"pc": 32820, "addr": 28673, "value": 91, "size_bytes": 1},
            ]
        else:
            assert execution["status"] == "fail"
            assert (
                execution["reason"]
                == "signature mismatch: expected [90, 92], observed [90, 91]"
            )
        runtime = [c for c in r["commands"] if c["name"] == "runtime"]
        assert len(runtime) == 1 and runtime[0]["returncode"] == 0


def audit(evidence: Path = HERE / "evidence") -> dict:
    hashes = json.loads((evidence / "hashes.json").read_text())
    assert hashes
    for name, digest in hashes.items():
        assert sha((evidence / name).read_bytes()) == digest
    result = json.loads((evidence / "result.json").read_text())
    assert result["schema_version"] == 1 and result["qualified"] is False
    assert result["source_commit"] == SOURCE
    for name, digest in result["source_sha256"].items():
        assert sha((HERE.parents[2] / name).read_bytes()) == digest
    for name in ["red.log", "native-red.log"]:
        log = (evidence / name).read_text()
        assert "test result: FAILED. 0 passed; 1 failed;" in log
        assert "left:" in log and "right:" in log
    tests = (evidence / "final-tests.log").read_text()
    assert not re.search(r"test result: FAILED|\.\.\. FAILED", tests)
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", tests))) == 474
    assert (
        sum(
            map(
                int,
                re.findall(
                    r"test result: ok\. \d+ passed; \d+ failed; (\d+) ignored;", tests
                ),
            )
        )
        == 63
    )
    assert (
        "read_track_id_crc_survives_native_checkpoints_and_interior_overrun ... ok"
        in tests
    )
    assert "Finished `release` profile" in (evidence / "build.log").read_text()
    assert "Finished `dev` profile" in (evidence / "final-clippy.log").read_text()
    audit_runtime(unpack(evidence / "runtime-rom.tar.gz"))
    spec = importlib.util.spec_from_file_location(
        "reference_verify", HERE / "reference" / "verify.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    reference = unpack(evidence / "stock-fuse.tar.gz")
    reference_result = module.audit(json.loads(reference["campaign.json"]), reference)
    assert reference_result["observations"] == 32
    return {
        "qualified": False,
        "checks_passed": True,
        "tests_passed": 474,
        "tests_ignored": 63,
        "fresh_rom_runs": 2,
        "wrong_signature_rejected": True,
    }


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2))
