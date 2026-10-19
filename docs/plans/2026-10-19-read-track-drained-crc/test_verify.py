# SPDX-License-Identifier: GPL-2.0-or-later
"""Frozen-evidence checks must reject misleading controller observations."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tarfile
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("drained_auditor", HERE / "verify.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        folder = tempfile.TemporaryDirectory(prefix="198x-drained-control-")
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        with tarfile.open(HERE / "evidence.tar.gz") as archive:
            archive.extractall(self.root, filter="data")
        self.record = json.loads((self.root / "campaign.json").read_text())

    def save(self) -> None:
        (self.root / "campaign.json").write_text(json.dumps(self.record) + "\n")
        self.rehash()

    def rehash(self) -> None:
        # Semantic controls remain red even when artifact hashes are recomputed.
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
        with self.assertRaisesRegex(ValueError, reason):
            module.audit(self.root)

    def test_retained_campaign(self) -> None:
        result = module.audit(self.root)
        self.assertEqual(result["observations"], 78)
        self.assertFalse(result["qualified"])
        self.assertEqual(
            sum(not c["mame_id_crc_comparable"] for c in result["comparisons"]), 5
        )

    def test_zero_payload(self) -> None:
        self.record["observations"][0]["observed"]["data"] = []
        self.save()
        self.reject("incomplete or wrong-order payload")

    def test_missing_observation(self) -> None:
        self.record["observations"].pop()
        self.save()
        self.reject("missing or duplicate observations")

    def test_duplicate_observation(self) -> None:
        self.record["observations"][-1] = self.record["observations"][0]
        self.save()
        self.reject("missing or duplicate observations")

    def test_truncated_result(self) -> None:
        self.record["observations"][0]["observed"]["result"].pop()
        self.save()
        self.reject("incomplete seven-byte result")

    def test_missing_recovery(self) -> None:
        self.record["observations"][0]["observed"]["recovery_msr"] = 208
        self.save()
        self.reject("missing next-command recovery")

    def test_false_qualification(self) -> None:
        self.record["qualified"] = True
        self.save()
        self.reject("false qualification or replay")

    def test_invented_mame_identity(self) -> None:
        self.record["mame_executable"]["source_commit"] = "0" * 40
        self.save()
        self.reject("invented MAME source identity")

    def test_patched_reference(self) -> None:
        self.record["fuse_source"]["patched"] = True
        self.save()
        self.reject("patched or different Fuse")

    def test_false_status_agreement(self) -> None:
        row = next(
            r
            for r in self.record["observations"]
            if r["engine"] == "fuse" and r["kind"] == 2
        )
        row["observed"]["result"] = [64, 160, 32, 0, 0, 1, 2]
        self.save()
        self.reject("nonrepeatable observation|changed captured status")

    def test_clean_crc_cannot_be_labelled_corrupt(self) -> None:
        row = next(
            r
            for r in self.record["observations"]
            if r["engine"] == "fuse" and r["kind"] == 1
        )
        clean = next(
            r
            for r in self.record["observations"]
            if r["engine"] == "fuse"
            and r["kind"] == 0
            and r["order"] == row["order"]
            and r["repeat"] == row["repeat"]
        )
        path = self.root / row["media"]
        path.write_bytes((self.root / clean["media"]).read_bytes())
        row["media_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.save()
        self.reject("wrong independent ID CRC")

    def test_missing_clock_corruption(self) -> None:
        row = self.record["observations"][0]
        path = self.root / row["media"]
        data = bytearray(path.read_bytes())
        data[6253 + 92 // 8] ^= 1 << (92 % 8)
        path.write_bytes(data)
        row["media_sha256"] = hashlib.sha256(data).hexdigest()
        self.save()
        self.reject("wrong missing-clock marks")

    def test_empty_artifact_manifest(self) -> None:
        (self.root / "hashes.json").write_text("{}")
        self.reject("missing artifact manifest")

    def test_host_timeout_cannot_pass(self) -> None:
        self.record["commands"][0]["timed_out"] = True
        self.save()
        self.reject("failed or unbounded command")


if __name__ == "__main__":
    unittest.main()
