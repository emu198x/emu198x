#!/usr/bin/env python3
"""Regression tests for raw sprite-phase measurement."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

from capture_record import PROFILES, measure_field  # noqa: E402


BACKGROUND = bytes((17, 0, 0, 255))
BLANK = bytes((0, 0, 0, 0))


def synthetic_field(
    profile: str,
    *,
    width: int = 24,
    height: int = 2,
    sample_row: int = 1,
) -> bytes:
    pixels = [BACKGROUND] * (width * height)
    row = sample_row * width
    pixels[row : row + 2] = [BLANK, BLANK]
    pixels[row + 6 : row + 8] = [PROFILES[profile]["marker_bgra"]] * 2
    pixels[row + 12 : row + 20] = [PROFILES[profile]["sprite_bgra"]] * 8
    return b"".join(pixels)


class SpritePhaseMeasurementTests(unittest.TestCase):
    def test_measures_ocs_intervals_and_signed_deltas(self) -> None:
        result = measure_field(
            synthetic_field("ocs"),
            "ocs",
            width=24,
            height=2,
            sample_row=1,
        )
        self.assertEqual(result["hblank_stop_sample"], 2)
        self.assertEqual(
            result["marker"],
            {"status": "observed", "start_sample": 6, "stop_sample": 8},
        )
        self.assertEqual(
            result["sprite"],
            {"status": "observed", "start_sample": 12, "stop_sample": 20},
        )
        self.assertEqual(result["sprite_start_minus_hblank_stop_samples"], 10)
        self.assertEqual(result["sprite_start_minus_marker_start_samples"], 6)
        self.assertEqual(result["uncertainty_samples"], 0)

    def test_measures_aga_palette_values(self) -> None:
        result = measure_field(
            synthetic_field("aga"),
            "aga",
            width=24,
            height=2,
            sample_row=1,
        )
        self.assertEqual(result["marker"]["start_sample"], 6)
        self.assertEqual(result["sprite"]["start_sample"], 12)

    def test_rejects_missing_leading_hblank(self) -> None:
        field = synthetic_field("ocs").replace(BLANK, BACKGROUND)
        with self.assertRaisesRegex(ValueError, "leading transparent-black"):
            measure_field(field, "ocs", width=24, height=2, sample_row=1)

    def test_rejects_multiple_sprite_intervals(self) -> None:
        field = bytearray(synthetic_field("ocs"))
        pixel = PROFILES["ocs"]["sprite_bgra"]
        start = (24 + 22) * 4
        field[start : start + 4] = pixel
        with self.assertRaisesRegex(ValueError, "2 sprite intervals"):
            measure_field(bytes(field), "ocs", width=24, height=2, sample_row=1)

    def test_rejects_wrong_field_size(self) -> None:
        with self.assertRaisesRegex(ValueError, "raw field has"):
            measure_field(b"", "ocs", width=24, height=2, sample_row=1)


if __name__ == "__main__":
    unittest.main()
