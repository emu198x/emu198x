"""Rehashed false observations must not become a passing captured packet."""

from __future__ import annotations

import hashlib
import json
import shutil
import tempfile
import unittest
from collections.abc import Callable
from pathlib import Path

from verify import audit

HERE = Path(__file__).resolve().parent


class PacketControls(unittest.TestCase):
    def reject(self, name: str, change: Callable[[dict], None], reason: str) -> None:
        with tempfile.TemporaryDirectory(prefix="format-packet-control-") as tmp:
            packet = Path(tmp) / "packet"
            shutil.copytree(HERE, packet)
            path = packet / name
            record = json.loads(path.read_text())
            change(record)
            path.write_text(json.dumps(record) + "\n")
            if name != "verification.json":
                manifest_path = packet / "verification.json"
                manifest = json.loads(manifest_path.read_text())
                manifest["artifact_hashes"][name] = hashlib.sha256(
                    path.read_bytes()
                ).hexdigest()
                manifest_path.write_text(json.dumps(manifest) + "\n")
            with self.assertRaisesRegex(ValueError, reason):
                audit(packet)

    def test_retained_packet(self) -> None:
        self.assertEqual(
            audit(HERE),
            {
                "packet_checks": "pass",
                "cases_per_run": 328,
                "runs": 2,
                "qualified": False,
            },
        )

    def test_zero_cases(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record.update(cases=[], case_count=0),
            "nonzero exact case count",
        )

    def test_duplicate_replaces_required_case(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record["cases"].__setitem__(0, record["cases"][1]),
            "duplicate cases",
        )

    def test_wrong_success_chrn(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record["cases"][0]["result_observed"].__setitem__(5, 11),
            "observed success CHRN",
        )

    def test_wrong_observed_protection_status(self) -> None:
        def change(record: dict) -> None:
            case = next(case for case in record["cases"] if case["action"] == "protect")
            case["result_observed"][0] = 0

        self.reject("run-1/format-native.json", change, "observed status")

    def test_forged_expected_status(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record["cases"][0].update(status_expected=[0x40, 2, 0]),
            "independent expected status",
        )

    def test_wrong_mode_identity(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record["cases"][0].update(nd=True),
            "ND identity",
        )

    def test_false_source_preservation(self) -> None:
        self.reject(
            "run-1/format-native.json",
            lambda record: record["cases"][0].update(source_unchanged=False),
            "source_unchanged",
        )

    def test_false_success_exit(self) -> None:
        def change(record: dict) -> None:
            row = next(
                row for row in record["commands"] if row["log"] == "wrong-result.log"
            )
            row["exit_code"] = 0

        self.reject("verification.json", change, "command outcome: wrong-result.log")


if __name__ == "__main__":
    unittest.main()
