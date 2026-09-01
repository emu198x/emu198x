#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Tests for Copper colour phase capture semantic validation."""

from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

from validate_capture import (  # noqa: E402
    CaptureValidationError,
    validate_capture_record,
    verify_bound_files,
)


def suite_fixture() -> dict[str, object]:
    return {
        "suite": {
            "id": "org.198x.amiga.copper-color-output-phase",
            "version": "1.0.0",
        },
        "cases": [
            {
                "id": "adjacent-color00-moves",
                "numeric_id": 1,
                "applicability": {
                    "chipsets": ["OCS"],
                    "regions": ["PAL"],
                    "min_chip_ram_bytes": 524288,
                },
                "geometry": {"sample_beam_line": 132},
                "color_program": {
                    "guard_word": "0x0011",
                    "move_words": ["0x0f00", "0x00f0", "0x000f", "0x0ff0"],
                },
                "capture": {
                    "ready_record_address": "0x0002ff00",
                    "ready_magic": "CCPH",
                    "byte_order": "big-endian",
                    "settle_fields": 8,
                    "capture_fields": 3,
                    "adjacent_field_stability_required": True,
                },
            }
        ],
        "artifacts": [
            {
                "case_id": "adjacent-color00-moves",
                "adf_file": "adjacent-color00-moves.adf",
                "payload_file": "adjacent-color00-moves.bin",
                "sha256": {"adf": "aa" * 32, "payload": "bb" * 32},
            }
        ],
    }


def valid_capture(suite: dict[str, object]) -> dict[str, object]:
    artifact = suite["artifacts"][0]
    sample_rows = []
    field_observations = []
    words = ["0x0011", "0x0f00", "0x00f0", "0x000f", "0x0ff0"]
    samples = [300, 308, 316, 324]
    for field in (8, 9, 10):
        sample_rows.append({"field": field, "capture_row": 132})
        transitions = []
        for ordinal, sample in enumerate(samples):
            transitions.append(
                {
                    "ordinal": ordinal,
                    "from_word": words[ordinal],
                    "to_word": words[ordinal + 1],
                    "status": "observed",
                    "sample": sample,
                    "minus_marker_start_samples": sample - 200,
                }
            )
        field_observations.append(
            {
                "field": field,
                "sample_row": 132,
                "marker": {
                    "status": "observed",
                    "start_sample": 200,
                    "stop_sample": 202,
                },
                "transitions": transitions,
                "adjacent_transition_deltas_samples": [8, 8, 8],
                "uncertainty_samples": 0,
            }
        )

    return {
        "schema_version": "1.0.0",
        "suite_id": suite["suite"]["id"],
        "suite_version": suite["suite"]["version"],
        "case_id": "adjacent-color00-moves",
        "artifact": {
            "adf_file": artifact["adf_file"],
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": artifact["payload_file"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "kind": "software-emulator",
            "product": "Example",
            "version": "1",
            "revision": "example-revision",
            "source_url": "https://example.invalid/source",
            "implementation_family": "example-family",
        },
        "machine": {
            "model": "Amiga 500",
            "cpu": "68000",
            "agnus_or_alice": "OCS Agnus",
            "denise_or_lisa": "OCS Denise",
            "chipset": "OCS",
            "region": "PAL",
            "chip_ram_bytes": 524288,
            "firmware": {"revision": "example", "sha256": "00" * 32},
        },
        "execution": {
            "cold_boot": True,
            "command_or_procedure": "example capture",
            "configuration_file_name": "configuration.txt",
            "configuration_sha256": "11" * 32,
            "ready_rule": {
                "record_address": "0x0002ff00",
                "magic": "CCPH",
                "case_number": 1,
                "schema_version": 1,
                "field_counter_minimum": 8,
                "byte_order": "big-endian",
            },
            "ready_observed_field": 8,
            "settle_fields": 8,
            "captured_fields": [8, 9, 10],
            "adjacent_field_stability": "confirmed",
        },
        "source_capture": {
            "method": "example",
            "width": 768,
            "height": 576,
            "pixel_format": "RGBA8",
            "stride_bytes": 3072,
            "blanking_retained": True,
            "overscan_retained": True,
            "filter": "none",
            "scaling": "none",
            "shader": "none",
            "automatic_crop": False,
            "file_name": "capture.rgba",
            "file_sha256": "22" * 32,
            "decoded_pixel_file_name": "capture.decoded.rgba",
            "decoded_pixel_sha256": "33" * 32,
        },
        "normalization": {
            "beam_mapping": {
                "sample_beam_line": 132,
                "sample_rows": sample_rows,
                "horizontal_origin_description": "retained raw origin",
                "samples_per_lores_pixel_numerator": 2,
                "samples_per_lores_pixel_denominator": 1,
            },
            "measurement_crop": None,
            "field_handling": "separate-fields",
            "color_conversion": "none",
            "alignment_search": False,
        },
        "observations": {
            "coordinate_unit": "source-capture sample",
            "marker_interval_convention": "start-inclusive-stop-exclusive",
            "transition_convention": "first sample rendered with the new COLOR00 word",
            "measurement_method": "exact digital colour transition",
            "fields": field_observations,
            "notes": [],
        },
        "provenance": {
            "operator": "Example",
            "capture_date": "2026-08-13",
            "host": "example-host",
            "classification": "software-derived",
        },
    }


class CaptureSemanticValidationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.suite = suite_fixture()

    def test_valid_record_passes(self) -> None:
        validate_capture_record(valid_capture(self.suite), self.suite)

    def test_rejects_non_adjacent_fields(self) -> None:
        capture = valid_capture(self.suite)
        capture["execution"]["captured_fields"] = [8, 10, 11]
        with self.assertRaisesRegex(CaptureValidationError, "ordered adjacent"):
            validate_capture_record(capture, self.suite)

    def test_rejects_wrong_profile(self) -> None:
        capture = valid_capture(self.suite)
        capture["machine"]["chipset"] = "ECS"
        with self.assertRaisesRegex(CaptureValidationError, "outside suite"):
            validate_capture_record(capture, self.suite)

    def test_rejects_observation_row_that_differs_from_beam_mapping(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0]["sample_row"] = 133
        with self.assertRaisesRegex(CaptureValidationError, "does not match"):
            validate_capture_record(capture, self.suite)

    def test_rejects_transition_colour_substitution(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0]["transitions"][1]["to_word"] = "0x0fff"
        with self.assertRaisesRegex(CaptureValidationError, "colour words"):
            validate_capture_record(capture, self.suite)

    def test_rejects_derived_marker_delta_that_does_not_match(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0]["transitions"][0][
            "minus_marker_start_samples"
        ] = 99
        with self.assertRaisesRegex(CaptureValidationError, "measured endpoints"):
            validate_capture_record(capture, self.suite)

    def test_rejects_adjacent_spacing_that_does_not_match(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0][
            "adjacent_transition_deltas_samples"
        ] = [8, 7, 8]
        with self.assertRaisesRegex(CaptureValidationError, "measured edges"):
            validate_capture_record(capture, self.suite)

    def test_accepts_unobserved_intermediate_colour(self) -> None:
        capture = valid_capture(self.suite)
        transition = capture["observations"]["fields"][0]["transitions"][1]
        transition.update(
            {
                "status": "not-observed",
                "sample": None,
                "minus_marker_start_samples": None,
            }
        )
        capture["observations"]["fields"][0][
            "adjacent_transition_deltas_samples"
        ] = [None, None, 8]
        validate_capture_record(capture, self.suite)

    def test_rejects_provenance_that_upgrades_software_to_hardware(self) -> None:
        capture = valid_capture(self.suite)
        capture["provenance"]["classification"] = "hardware-evidence"
        with self.assertRaisesRegex(CaptureValidationError, "contradicts"):
            validate_capture_record(capture, self.suite)

    def test_bound_file_checks_hash_all_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture_root = root / "capture"
            suite_root = root / "suite"
            capture_root.mkdir()
            suite_root.mkdir()
            files = {
                capture_root / "capture.rgba": b"raw",
                capture_root / "capture.decoded.rgba": b"decoded",
                capture_root / "configuration.txt": b"configuration",
                suite_root / "adjacent-color00-moves.adf": b"adf",
                suite_root / "adjacent-color00-moves.bin": b"payload",
            }
            for path, content in files.items():
                path.write_bytes(content)

            suite = suite_fixture()
            suite["artifacts"][0]["sha256"] = {
                "adf": hashlib.sha256(b"adf").hexdigest(),
                "payload": hashlib.sha256(b"payload").hexdigest(),
            }
            capture = valid_capture(suite)
            capture["source_capture"]["file_sha256"] = hashlib.sha256(b"raw").hexdigest()
            capture["source_capture"]["decoded_pixel_sha256"] = hashlib.sha256(
                b"decoded"
            ).hexdigest()
            capture["execution"]["configuration_sha256"] = hashlib.sha256(
                b"configuration"
            ).hexdigest()

            validate_capture_record(capture, suite)
            verify_bound_files(
                capture_root / "capture.json", suite_root / "suite-v1.json", capture
            )

            (capture_root / "capture.rgba").write_bytes(b"changed")
            with self.assertRaisesRegex(CaptureValidationError, "raw capture SHA-256"):
                verify_bound_files(
                    capture_root / "capture.json",
                    suite_root / "suite-v1.json",
                    capture,
                )


if __name__ == "__main__":
    unittest.main()
