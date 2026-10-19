# SPDX-License-Identifier: GPL-2.0-or-later
"""Adversarial completeness and byte-level controls; no dependency on pytest."""

from __future__ import annotations

import copy
import json
import tarfile
import tempfile
import unittest
from pathlib import Path

import verify

HERE = Path(__file__).resolve().parent


class AuditControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="198x-field-controls-")
        cls.root = Path(cls.temp.name)
        with tarfile.open(HERE / "evidence.tar.gz") as archive:
            archive.extractall(cls.root, filter="data")
        cls.original = {
            p.name: p.read_bytes() for p in cls.root.iterdir() if p.is_file()
        }
        cls.expect = json.loads((HERE / "expectations.json").read_text())

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def setUp(self) -> None:
        for name, data in self.original.items():
            (self.root / name).write_bytes(data)

    def campaign(self, change):
        path = self.root / "campaign.json"
        record = json.loads(path.read_text())
        change(record)
        path.write_text(json.dumps(record))
        self.rehash(path.name)

    def rehash(self, name: str) -> None:
        path = self.root / "hashes.json"
        h = json.loads(path.read_text())
        h[name] = verify.digest((self.root / name).read_bytes())
        path.write_text(json.dumps(h))

    def check_bad(self) -> None:
        with self.assertRaises(ValueError):
            verify.audit(self.root, self.expect)

    def test_baseline(self) -> None:
        self.assertGreater(
            verify.audit(self.root, self.expect)["byte_and_state_assertions"], 0
        )

    def test_missing_row(self) -> None:
        self.campaign(lambda d: d["observations"].pop())
        self.check_bad()

    def test_duplicate_row(self) -> None:
        self.campaign(lambda d: d["observations"].append(d["observations"][0]))
        self.check_bad()

    def test_empty_rows(self) -> None:
        self.campaign(lambda d: d.update(observations=[]))
        self.check_bad()

    def test_replay_claim(self) -> None:
        self.campaign(
            lambda d: d["observations"][0].update(
                evidence_class="captured_reference_replay"
            )
        )
        self.check_bad()

    def test_result_truncated(self) -> None:
        self.campaign(lambda d: d["observations"][0]["observed"]["result"].pop())
        self.check_bad()

    def test_payload_truncated(self) -> None:
        self.campaign(lambda d: d["observations"][0]["observed"]["data"].pop())
        self.check_bad()

    def test_lost_recovery(self) -> None:
        self.campaign(
            lambda d: d["observations"][0]["observed"].update(recovery_msr=208)
        )
        self.check_bad()

    def test_mutated_medium(self) -> None:
        self.campaign(
            lambda d: d["observations"][0]["observed"].update(media_unchanged=False)
        )
        self.check_bad()

    def test_qualified_claim(self) -> None:
        self.campaign(lambda d: d.update(qualified=True))
        self.check_bad()

    def test_pass_disguised_as_failure(self) -> None:
        self.campaign(
            lambda d: d["observations"][0].update(
                status="fail", reason="pretend failed"
            )
        )
        self.check_bad()

    def test_wrong_expected_size(self) -> None:
        expected = copy.deepcopy(self.expect)
        expected["emu:n0"]["payload_bytes"] += 1
        with self.assertRaisesRegex(ValueError, "expected.*size"):
            verify.audit(self.root, expected)

    def test_wrong_expected_result(self) -> None:
        expected = copy.deepcopy(self.expect)
        expected["emu:n0"]["result"][0] ^= 64
        with self.assertRaisesRegex(ValueError, "expected result"):
            verify.audit(self.root, expected)

    def test_raw_crc_rehashed(self) -> None:
        p = self.root / "n0.raw"
        data = bytearray(p.read_bytes())
        data[3 + 100] ^= 1
        p.write_bytes(data)
        self.rehash(p.name)
        with self.assertRaisesRegex(ValueError, "field CRC"):
            verify.audit(self.root, self.expect)

    def test_raw_payload_rehashed(self) -> None:
        p = self.root / "n0.raw"
        data = bytearray(p.read_bytes())
        data[3 + 150] ^= 1
        p.write_bytes(data)
        self.rehash(p.name)
        self.check_bad()

    def test_clock_rehashed(self) -> None:
        p = self.root / "n0.raw"
        data = bytearray(p.read_bytes())
        data[6253] ^= 1
        p.write_bytes(data)
        self.rehash(p.name)
        self.check_bad()

    def test_truncated_mfi_rehashed(self) -> None:
        p = self.root / "n0.mfi"
        p.write_bytes(p.read_bytes()[:-1])
        self.rehash(p.name)
        self.check_bad()

    def test_missing_artifact(self) -> None:
        (self.root / "n0.mfi").unlink()
        self.check_bad()

    def test_missing_manifest_entry(self) -> None:
        p = self.root / "hashes.json"
        h = json.loads(p.read_text())
        h.pop("n0.raw")
        p.write_text(json.dumps(h))
        self.check_bad()

    def test_changed_expected_matrix(self) -> None:
        self.campaign(lambda d: d["cases"][0]["command"].__setitem__(5, 2))
        self.check_bad()

    def test_zero_exit_without_assertions(self) -> None:
        self.campaign(lambda d: d["observations"][0].pop("observed"))
        with self.assertRaises((KeyError, ValueError)):
            verify.audit(self.root, self.expect)


if __name__ == "__main__":
    unittest.main()
