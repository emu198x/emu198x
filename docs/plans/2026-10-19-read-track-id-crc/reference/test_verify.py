#!/usr/bin/env python3
from __future__ import annotations

import json
import unittest
from pathlib import Path

from verify import audit, load_files, sha

HERE = Path(__file__).resolve().parent


class CaptureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.files = load_files()
        self.record = json.loads(self.files["campaign.json"])

    def case(self, mask: int, consumed: int) -> dict:
        return next(
            c
            for c in self.record["cases"]
            if c["repeat"] == 1 and c["crc_mask"] == mask and c["consumed"] == consumed
        )

    def test_complete(self) -> None:
        self.assertEqual(audit(self.record, self.files)["observations"], 32)

    def test_lost_observed_id_error(self) -> None:
        self.case(1, 1)["observed"]["result"][1] &= ~0x20
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_data_error_published_early(self) -> None:
        self.case(2, 1)["observed"]["result"][1] |= 0x20
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_data_field_error_published_early(self) -> None:
        self.case(1, 1)["observed"]["result"][2] |= 0x20
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_crc_control_is_actually_corrupt(self) -> None:
        case = self.case(1, 1)
        raw = bytearray(self.files[case["raw_path"]])
        raw[103] ^= 1
        self.files[case["raw_path"]] = bytes(raw)
        case["raw_sha256"] = sha(raw)
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_missing_repeat(self) -> None:
        self.record["cases"].pop()
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_false_qualification(self) -> None:
        self.record["qualified"] = True
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)

    def test_patch_masquerades_as_stock(self) -> None:
        self.record["fuse_source"]["patched"] = True
        with self.assertRaises(AssertionError):
            audit(self.record, self.files)


if __name__ == "__main__":
    unittest.main()
