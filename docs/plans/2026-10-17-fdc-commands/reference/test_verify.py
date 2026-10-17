# SPDX-License-Identifier: GPL-2.0-or-later
"""The reference audit must reject missing evidence and preserve failed semantics."""

import copy
import json
import unittest
from pathlib import Path

from verify import check, main


class ReferenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.record = json.loads(Path(__file__).with_name("final-1.json").read_text())

    def test_recorded_repeats(self) -> None:
        result = main()
        self.assertIs(result["qualified"], False)
        failures = [r for r in result["cases"] if r.get("status") == "fail"]
        self.assertGreater(len(failures), 0)

    def test_zero_exit_without_observation_rejects(self) -> None:
        del self.record["cases"][0]["observation"]
        with self.assertRaisesRegex(ValueError, "missing executed"):
            check(self.record)

    def test_missing_case_rejects(self) -> None:
        self.record["cases"].pop()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            check(self.record)

    def test_wrong_reference_rejects(self) -> None:
        self.record["mame"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "reference binary"):
            check(self.record)

    def test_changed_readback_is_not_a_mark_pass(self) -> None:
        forged = copy.deepcopy(self.record)
        forged["cases"][0]["observation"]["readback"]["result"][2] = 0x40
        with self.assertRaisesRegex(ValueError, "payload/mark"):
            check(forged)
