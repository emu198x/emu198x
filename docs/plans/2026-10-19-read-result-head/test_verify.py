#!/usr/bin/env python3
"""Negative controls for the bounded result-head evidence auditor."""

from __future__ import annotations

import io
import json
import tarfile
import unittest
from collections.abc import Callable
from pathlib import Path

from verify import archive_files, audit, audit_runtime, sha

HERE = Path(__file__).resolve().parent


def pack(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for name, data in files.items():
            member = tarfile.TarInfo(name)
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
    return buffer.getvalue()


class AuditTests(unittest.TestCase):
    def setUp(self) -> None:
        evidence = HERE / "evidence"
        self.files = {p.name: p.read_bytes() for p in evidence.iterdir() if p.is_file()}

    def change_reference(self, change: Callable[[dict], None]) -> None:
        files = archive_files(self.files["stock-fuse.tar.gz"])
        record = json.loads(files["campaign.json"])
        change(record)
        files["campaign.json"] = json.dumps(record).encode()
        self.files["stock-fuse.tar.gz"] = pack(files)

    def test_complete(self) -> None:
        self.assertEqual(audit(self.files)["reference_observations"], 36)

    def test_zero_reference_observations(self) -> None:
        self.change_reference(lambda r: r.update(cases=[]))
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_wrong_result_head(self) -> None:
        self.change_reference(
            lambda r: r["cases"][0]["observed"]["result"].__setitem__(4, 0)
        )
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_patched_reference(self) -> None:
        self.change_reference(lambda r: r["fuse_source"].update(patched=True))
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_false_qualification(self) -> None:
        r = json.loads(self.files["result.json"])
        r["qualified"] = True
        self.files["result.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_missing_snapshot_assertion(self) -> None:
        self.files["snapshot.log"] = b"test result: ok. 0 passed; 0 failed;"
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_green_exit_cannot_erase_red(self) -> None:
        r = json.loads(self.files["red-identities.json"])
        r["exit"] = 0
        self.files["red-identities.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit(self.files)

    def test_zero_runtime_tests(self) -> None:
        files = archive_files(self.files["runtime-rom.tar.gz"])
        files["198x-result-h-runtime-tests.log"] = (
            b"test result: ok. 0 passed; 0 failed;"
        )
        parent = json.loads(files["198x-result-h-parent-verification.json"])
        parent["commands"][0]["log_sha256"] = sha(
            files["198x-result-h-runtime-tests.log"]
        )
        files["198x-result-h-parent-verification.json"] = json.dumps(parent).encode()
        with self.assertRaises(AssertionError):
            audit_runtime(files)

    def test_false_execution_pass(self) -> None:
        files = archive_files(self.files["runtime-rom.tar.gz"])
        r = json.loads(files["rom-wrong/regression.json"])
        r["regression_passed"] = True
        r["stages"]["execution"]["status"] = "pass"
        files["rom-wrong/regression.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit_runtime(files)

    def test_zero_loading_assertions(self) -> None:
        files = archive_files(self.files["runtime-rom.tar.gz"])
        r = json.loads(files["rom-1/regression.json"])
        r["stages"]["loading"]["assertions"] = 0
        files["rom-1/regression.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit_runtime(files)

    def test_missing_execution_writes(self) -> None:
        files = archive_files(self.files["runtime-rom.tar.gz"])
        r = json.loads(files["rom-1/regression.json"])
        r["stages"]["execution"]["observed"]["writes"] = []
        files["rom-1/regression.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit_runtime(files)

    def test_wrong_source_identity(self) -> None:
        r = json.loads(self.files["result.json"])
        r["source_commit"] = "0" * 40
        self.files["result.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            audit(self.files)


if __name__ == "__main__":
    unittest.main()
