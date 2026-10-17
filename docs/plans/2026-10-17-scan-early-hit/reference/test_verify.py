# SPDX-License-Identifier: GPL-2.0-or-later
"""Corrupt observations deliberately; passing process exits cannot qualify them."""

from __future__ import annotations

import copy
import hashlib
import json
import tarfile
import tempfile
import unittest
from pathlib import Path

from verify import EVIDENCE, PINS, check, main


class EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="198x-scan-negative-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        with tarfile.open(EVIDENCE / "runs.tar.gz") as archive:
            archive.extractall(self.root, filter="data")
        self.record = json.loads((self.root / "campaign.json").read_text())

    def test_complete_archive(self) -> None:
        result = main()
        self.assertFalse(result["qualified"])
        self.assertEqual(result["fresh_observations"], 28)
        self.assertEqual(result["stock_reference_count_failures"], 6)

    def test_first_hit_wrong_byte_count_rejects(self) -> None:
        self.record["fuse"][0]["observation"]["sent"] = 1024
        with self.assertRaisesRegex(ValueError, "Fuse byte count"):
            check(self.record, self.root)

    def test_patched_reference_cannot_masquerade_as_stock(self) -> None:
        self.record["fuse"][0]["identity"] = copy.deepcopy(
            PINS["fuse_binaries"]["patched"]
        )
        with self.assertRaisesRegex(ValueError, "stock/patched identity"):
            check(self.record, self.root)

    def test_missing_result_rejects(self) -> None:
        self.record["fuse"][0]["observation"].pop("result")
        with self.assertRaises((KeyError, ValueError)):
            check(self.record, self.root)

    def test_false_qualification_rejects(self) -> None:
        self.record["qualified"] = True
        with self.assertRaisesRegex(ValueError, "false qualification"):
            check(self.record, self.root)

    def test_missing_reference_case_rejects(self) -> None:
        self.record["fuse"].pop()
        with self.assertRaisesRegex(ValueError, "missing Fuse"):
            check(self.record, self.root)

    def test_zero_emu_observations_rejects(self) -> None:
        self.record["emu"][0]["observations"] = []
        with self.assertRaisesRegex(ValueError, "wrong Emu"):
            check(self.record, self.root)

    def test_changed_fixture_rejects_even_when_rehashed(self) -> None:
        row = self.record["fuse"][0]
        path = self.root / row["track"]
        data = bytearray(path.read_bytes())
        data[143] ^= 1
        path.write_bytes(data)
        digest = hashlib.sha256(data).hexdigest()
        row["track_sha256"] = digest
        next(a for a in self.record["artifacts"] if a["path"] == path.name)[
            "sha256"
        ] = digest
        with self.assertRaisesRegex(ValueError, "fixture payloads"):
            check(self.record, self.root)

    def test_changed_source_identity_rejects(self) -> None:
        self.record["sources"]["host.c"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "changed identity"):
            check(self.record, self.root)

    def test_forged_suite_success_with_zero_tests_rejects(self) -> None:
        path = self.root / "controller-tests.stdout"
        path.write_text("test result: ok. 0 passed; 0 failed;\n")
        next(a for a in self.record["artifacts"] if a["path"] == path.name).update(
            size=path.stat().st_size,
            sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
        )
        with self.assertRaisesRegex(ValueError, "controller suite"):
            check(self.record, self.root)
