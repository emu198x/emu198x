#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Tests for Copper colour phase source validation."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import build as corpus_build  # noqa: E402


class SourceValidationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        _, cases = corpus_build.load_cases()
        cls.case = cases[0]

    def test_canonical_case_passes(self) -> None:
        corpus_build.validate_case(copy.deepcopy(self.case))

    def test_rejects_non_ocs_applicability(self) -> None:
        case = copy.deepcopy(self.case)
        case["applicability"]["chipsets"] = ["OCS", "ECS"]
        with self.assertRaisesRegex(corpus_build.BuildError, "OCS-only"):
            corpus_build.validate_case(case)

    def test_rejects_move_before_fetch_end(self) -> None:
        case = copy.deepcopy(self.case)
        case["geometry"]["move_wait_hpos_cck"] = 96
        with self.assertRaisesRegex(corpus_build.BuildError, "after the bitplane DMA"):
            corpus_build.validate_case(case)

    def test_rejects_missing_adjacent_move(self) -> None:
        case = copy.deepcopy(self.case)
        case["color_program"]["move_words"].pop()
        with self.assertRaisesRegex(corpus_build.BuildError, "exactly four"):
            corpus_build.validate_case(case)

    def test_generated_include_exposes_schedule_and_identity(self) -> None:
        include = corpus_build.generated_case_include(self.case)
        self.assertIn(".equ CASE_MOVE_COUNT, 4", include)
        self.assertIn(".equ CASE_MOVE_WAIT_HPOS_CCK, 144", include)
        self.assertIn("amiga-copper-color-phase-v1", include)


if __name__ == "__main__":
    unittest.main()
