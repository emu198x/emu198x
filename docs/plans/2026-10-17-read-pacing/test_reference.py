# SPDX-License-Identifier: GPL-2.0-or-later
"""Ensure the bounded reference checks reject missing and wrong observations."""

import copy
import unittest

from reference import check


class ReferenceChecks(unittest.TestCase):
    def observation(self):
        return {
            "data": [33] * 512,
            "times": [i * 0.000032 for i in range(512)],
            "after_read_msr": [48] * 511 + [208],
            "result": [64, 128, 0, 0, 0, 1, 2],
            "idle_msr": 128,
        }

    def test_complete(self):
        check(self.observation(), 0)

    def test_wrong_payload_status_cadence_and_request_are_rejected(self):
        for key, value in [
            ("data", []),
            ("times", []),
            ("after_read_msr", []),
            ("after_read_msr", [240] * 512),
            ("result", [0] * 7),
            ("idle_msr", 240),
            ("times", [0] * 512),
        ]:
            with self.subTest(
                key=key, value=value[:2] if isinstance(value, list) else value
            ):
                observed = copy.deepcopy(self.observation())
                observed[key] = value
                with self.assertRaises(ValueError):
                    check(observed, 0)

    def test_absent_overrun_is_rejected(self):
        with self.assertRaises(ValueError):
            check(self.observation(), 1000)
