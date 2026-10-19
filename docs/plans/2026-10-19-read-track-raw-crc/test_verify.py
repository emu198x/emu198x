# SPDX-License-Identifier: GPL-2.0-or-later
"""Negative controls rehash changed records so semantic guards must reject them."""

from __future__ import annotations

import hashlib
import json
import struct
import tarfile
import tempfile
import unittest
import zlib
from pathlib import Path

import verify
from decode import validate

HERE = Path(__file__).resolve().parent


class EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        folder = tempfile.TemporaryDirectory(prefix="198x-mfi-control-")
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        with tarfile.open(HERE / "evidence.tar.gz") as archive:
            archive.extractall(self.root, filter="data")
        self.record = json.loads((self.root / "campaign.json").read_text())

    def save(self) -> None:
        (self.root / "campaign.json").write_text(json.dumps(self.record))
        (self.root / "hashes.json").write_text(
            json.dumps(
                {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in self.root.iterdir()
                    if p.is_file() and p.name != "hashes.json"
                }
            )
        )

    def reject(self, reason: str) -> None:
        self.save()
        with self.assertRaisesRegex(ValueError, reason):
            verify.audit(self.root)

    def test_retained_campaign(self) -> None:
        result = verify.audit(self.root)
        self.assertEqual(result, json.loads((HERE / "summary.json").read_text()))
        self.assertEqual(len(result["comparisons"]), 13)
        self.assertFalse(result["qualified"])

    def test_missing_observation(self) -> None:
        self.record["observations"].pop()
        self.reject("missing or duplicate observations")

    def test_duplicate_observation(self) -> None:
        self.record["observations"][-1] = self.record["observations"][0]
        self.reject("missing or duplicate observations")

    def test_missing_fixture(self) -> None:
        self.record["fixtures"].pop()
        self.reject("missing or duplicate fixtures")

    def test_wrong_expected_payload(self) -> None:
        self.record["observations"][0]["expected_payload_bytes"] = 0
        self.reject("different probe invocation")

    def test_changed_output(self) -> None:
        self.record["observations"][0]["observed"]["data"] = []
        self.reject("observation differs from raw output")

    def change_observation(self, key: str, value: object) -> None:
        row = self.record["observations"][0]
        row["observed"][key] = value
        (self.root / (row["name"] + ".json")).write_text(json.dumps(row["observed"]))

    def test_zero_payload_even_with_matching_output(self) -> None:
        self.change_observation("data", [])
        self.reject("incomplete or wrong payload")

    def test_truncated_result(self) -> None:
        self.change_observation("result", [64, 128])
        self.reject("incomplete result")

    def test_wrong_recovery(self) -> None:
        self.change_observation("recovery_msr", 208)
        self.reject("missing next-command recovery")

    def test_timeout(self) -> None:
        self.record["commands"][2]["timed_out"] = True
        self.reject("failed or unbounded invocation")

    def test_false_qualification(self) -> None:
        self.record["qualified"] = True
        self.reject("false qualification")

    def test_false_fresh_replay(self) -> None:
        self.record["baseline"]["emu_fuse_fresh_this_study"] = True
        self.reject("baseline replay promoted")

    def test_invented_source_identity(self) -> None:
        self.record["inspected_mame_source"]["executed_source_identity"] = True
        self.reject("invented reference identity")

    def test_clean_raw_relabelled_as_id_crc(self) -> None:
        clean = self.record["fixtures"][0]
        bad = self.record["fixtures"][1]
        (self.root / bad["raw"]).write_bytes((self.root / clean["raw"]).read_bytes())
        self.reject("different audited raw fixture")

    def test_rehashed_changed_flux(self) -> None:
        fixture = self.record["fixtures"][0]
        path = self.root / fixture["media"]
        data = bytearray(path.read_bytes())
        stream = bytearray(zlib.decompress(data[48:]))
        # Shift one transition off the MFM clock grid; keep archive/hash coherent.
        struct.pack_into("<I", stream, 0, struct.unpack_from("<I", stream)[0] + 1)
        packed = zlib.compress(stream)
        struct.pack_into("<I", data, 36, len(packed))
        path.write_bytes(data[:48] + packed)
        fixture["media_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.reject("off-grid flux transition")

    def test_trailing_compressed_stream(self) -> None:
        f = self.record["fixtures"][0]
        data = bytearray((self.root / f["media"]).read_bytes())
        data.extend(zlib.compress(b"extra"))
        struct.pack_into("<I", data, 36, len(data) - 48)
        with self.assertRaisesRegex(ValueError, "trailing compressed"):
            validate(data, (self.root / f["raw"]).read_bytes())

    def test_decompression_bound(self) -> None:
        f = self.record["fixtures"][0]
        data = bytearray((self.root / f["media"]).read_bytes()[:48])
        packed = zlib.compress(bytes(400004))
        struct.pack_into("<II", data, 36, len(packed), 400004)
        with self.assertRaisesRegex(ValueError, "oversized"):
            validate(data + packed, (self.root / f["raw"]).read_bytes())

    def test_missing_raw_artifacts(self) -> None:
        (self.root / "hashes.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "missing raw artifacts"):
            verify.audit(self.root)


if __name__ == "__main__":
    unittest.main()
