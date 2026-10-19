# SPDX-License-Identifier: GPL-2.0-or-later
"""Corrupt the evidence and expectations after rehashing; checks must go red."""

from __future__ import annotations

import copy
import json
import tarfile
import tempfile
import unittest
from pathlib import Path

import verify

HERE = Path(__file__).resolve().parent


class FramingControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="198x-opcode-negative-")
        cls.root = Path(cls.temp.name)
        with tarfile.open(HERE / "evidence.tar.gz") as a:
            a.extractall(cls.root, filter="data")
        cls.original = {
            p.name: p.read_bytes() for p in cls.root.iterdir() if p.is_file()
        }
        cls.expected = json.loads((HERE / "expectations.json").read_text())

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def setUp(self) -> None:
        for name, data in self.original.items():
            (self.root / name).write_bytes(data)

    def change(self, callback) -> None:
        p = self.root / "campaign.json"
        d = json.loads(p.read_text())
        callback(d)
        p.write_text(json.dumps(d))
        self.rehash(p.name)

    def rehash(self, name: str) -> None:
        p = self.root / "hashes.json"
        h = json.loads(p.read_text())
        h[name] = verify.sha((self.root / name).read_bytes())
        p.write_text(json.dumps(h))

    def bad(self) -> None:
        with self.assertRaises(ValueError):
            verify.audit(self.root, self.expected)

    def test_baseline(self) -> None:
        self.assertEqual(
            verify.audit(self.root, self.expected)["early_rejection_captures"], 12
        )

    def test_missing_row(self) -> None:
        self.change(lambda d: d["observations"].pop())
        self.bad()

    def test_duplicate_row(self) -> None:
        self.change(lambda d: d["observations"].append(d["observations"][0]))
        self.bad()

    def test_zero_rows(self) -> None:
        self.change(lambda d: d.update(observations=[]))
        self.bad()

    def test_wrong_command_length(self) -> None:
        self.change(
            lambda d: d["observations"][0]["observed"].update(command_bytes_sent=1)
        )
        self.bad()

    def test_rejection_padded_to_seven(self) -> None:
        def mutate(d):
            row = next(
                r
                for r in d["observations"]
                if r.get("observed", {}).get("command_bytes_sent") == 1
            )
            row["observed"]["result"] += [0] * 6

        self.change(mutate)
        self.bad()

    def test_unsent_parameters_claimed_sent(self) -> None:
        def mutate(d):
            row = next(
                r
                for r in d["observations"]
                if r.get("observed", {}).get("command_bytes_sent") == 1
            )
            row["observed"]["command_bytes_sent"] = 9

        self.change(mutate)
        self.bad()

    def test_truncated_result(self) -> None:
        self.change(lambda d: d["observations"][0]["observed"]["result"].pop())
        self.bad()

    def test_truncated_payload(self) -> None:
        self.change(lambda d: d["observations"][0]["observed"]["data"].pop())
        self.bad()

    def test_lost_recovery(self) -> None:
        self.change(lambda d: d["observations"][0]["observed"].update(recovery_msr=208))
        self.bad()

    def test_media_changed(self) -> None:
        self.change(
            lambda d: d["observations"][0]["observed"].update(media_unchanged=False)
        )
        self.bad()

    def test_replay_claimed_fresh(self) -> None:
        self.change(
            lambda d: d["observations"][0].update(
                evidence_class="captured_reference_replay"
            )
        )
        self.bad()

    def test_missing_entry_claimed_pass(self) -> None:
        def mutate(d):
            next(
                r for r in d["observations"] if r["status"] == "not_applicable"
            ).update(status="pass")

        self.change(mutate)
        self.bad()

    def test_wrong_expected_result(self) -> None:
        e = copy.deepcopy(self.expected)
        e["emu:ids-12-op-42"]["result"][0] ^= 64
        with self.assertRaisesRegex(ValueError, "expected framing/result"):
            verify.audit(self.root, e)

    def test_wrong_expected_size(self) -> None:
        e = copy.deepcopy(self.expected)
        e["emu:ids-12-op-42"]["payload_bytes"] -= 1
        with self.assertRaisesRegex(ValueError, "expected payload/size"):
            verify.audit(self.root, e)

    def test_raw_crc_after_rehash(self) -> None:
        p = self.root / "ids-12-op-42.raw"
        b = bytearray(p.read_bytes())
        b[103] ^= 1
        p.write_bytes(b)
        self.rehash(p.name)
        with self.assertRaisesRegex(ValueError, "field CRC"):
            verify.audit(self.root, self.expected)

    def test_raw_clock_after_rehash(self) -> None:
        p = self.root / "ids-12-op-42.raw"
        b = bytearray(p.read_bytes())
        b[6253] ^= 1
        p.write_bytes(b)
        self.rehash(p.name)
        self.bad()


if __name__ == "__main__":
    unittest.main()
