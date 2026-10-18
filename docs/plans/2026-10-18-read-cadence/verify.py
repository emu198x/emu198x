#!/usr/bin/env python3
"""Audit retained cadence evidence without ROMs or emulator binaries."""
from __future__ import annotations

import hashlib
import json
import tarfile
from pathlib import Path


def verify(root: Path) -> None:
    record = json.loads((root / "result.json").read_text())
    assert record["schema_version"] == 1
    assert record["qualified"] is False
    assert record["test_results"]["passed"] > 0
    assert record["test_results"]["failed"] == 0
    archive = root / "runs.tar.gz"
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == record["archive_sha256"]
    with tarfile.open(archive) as bundle:
        members = bundle.getmembers()
        assert len(members) == len(record["members"])
        assert all(m.isfile() and not m.name.startswith("/") and ".." not in Path(m.name).parts for m in members)
        assert {m.name for m in members} == set(record["members"])
        for member in members:
            content = bundle.extractfile(member)
            assert content is not None
            assert hashlib.sha256(content.read()).hexdigest() == record["members"][member.name]
        for name in ["1", "2", "wrong"]:
            stream = bundle.extractfile(f"{name}/regression.json")
            assert stream is not None
            run = json.load(stream)
            assert run["qualified"] is False
            assert run["tools"]["emu"]["commit"] == record["source_commit"]
            assert run["tools"]["emu"]["sha256"] == record["binary_sha256"]
            assert run["stages"] == record["runs"][name]["stages"]
            assert run["regression_passed"] is (name != "wrong")
            for stage in ["native_format", "firmware", "loading"]:
                assert run["stages"][stage]["status"] == "pass"
                assert run["stages"][stage]["assertions"] > 0
            execution = run["stages"]["execution"]
            if name == "wrong":
                assert execution["status"] == "fail"
                assert run["failure"] == "signature mismatch: expected [90, 92], observed [90, 91]"
            else:
                assert execution["status"] == "pass" and execution["assertions"] > 0
                assert execution["observed"]["signature_before"] == [0, 0]
                assert execution["observed"]["signature_after"] == execution["expected"]["signature"] == [90, 91]
                assert execution["observed"]["screen_sha256"] == execution["expected"]["screen_sha256"]
    print("Cadence regression evidence passes; full qualification remains false")


if __name__ == "__main__":
    verify(Path(__file__).resolve().parent)
