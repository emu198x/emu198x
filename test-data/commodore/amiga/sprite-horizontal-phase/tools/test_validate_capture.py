#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Tests for sprite-phase capture semantic validation."""

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
            "id": "org.198x.amiga.sprite-horizontal-phase",
            "version": "1.0.0",
        },
        "cases": [
            {
                "id": "fixed-lores-sprite",
                "numeric_id": 1,
                "applicability": {
                    "chipsets": ["OCS", "ECS", "AGA"],
                    "regions": ["PAL"],
                    "min_chip_ram_bytes": 524288,
                },
                "capture": {
                    "ready_record_address": "0x0002ff00",
                    "ready_magic": "SPHX",
                    "byte_order": "big-endian",
                    "settle_fields": 8,
                    "capture_fields": 3,
                    "adjacent_field_stability_required": True,
                },
            }
        ],
        "artifacts": [
            {
                "case_id": "fixed-lores-sprite",
                "adf_file": "fixed-lores-sprite.adf",
                "payload_file": "fixed-lores-sprite.bin",
                "sha256": {"adf": "aa" * 32, "payload": "bb" * 32},
            }
        ],
    }


def valid_capture(suite: dict[str, object]) -> dict[str, object]:
    artifact = suite["artifacts"][0]
    observations = []
    sample_rows = []
    for field in (8, 9, 10):
        sample_rows.append({"field": field, "capture_row": 132})
        observations.append(
            {
                "field": field,
                "sample_row": 132,
                "hblank_status": "observed",
                "hblank_stop_sample": 40,
                "marker": {
                    "status": "observed",
                    "start_sample": 100,
                    "stop_sample": 102,
                },
                "sprite": {
                    "status": "observed",
                    "start_sample": 200,
                    "stop_sample": 232,
                },
                "sprite_start_minus_hblank_stop_samples": 160,
                "sprite_start_minus_marker_start_samples": 100,
                "uncertainty_samples": 0,
            }
        )

    return {
        "schema_version": "1.0.0",
        "suite_id": suite["suite"]["id"],
        "suite_version": suite["suite"]["version"],
        "case_id": "fixed-lores-sprite",
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
            "agnus_or_alice": "Agnus",
            "denise_or_lisa": "Denise",
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
                "magic": "SPHX",
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
            "interval_convention": "start-inclusive-stop-exclusive",
            "measurement_method": "exact digital transition",
            "fields": observations,
            "notes": [],
        },
        "provenance": {
            "operator": "Example",
            "capture_date": "2026-08-08",
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

    def test_rejects_observation_row_that_differs_from_beam_mapping(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0]["sample_row"] = 133
        with self.assertRaisesRegex(CaptureValidationError, "does not match"):
            validate_capture_record(capture, self.suite)

    def test_rejects_invalid_interval(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0]["sprite"]["stop_sample"] = 200
        with self.assertRaisesRegex(CaptureValidationError, "start_sample < stop_sample"):
            validate_capture_record(capture, self.suite)

    def test_rejects_derived_delta_that_does_not_match_endpoints(self) -> None:
        capture = valid_capture(self.suite)
        capture["observations"]["fields"][0][
            "sprite_start_minus_marker_start_samples"
        ] = 99
        with self.assertRaisesRegex(CaptureValidationError, "measured endpoints"):
            validate_capture_record(capture, self.suite)

    def test_rejects_provenance_that_upgrades_software_to_hardware(self) -> None:
        capture = valid_capture(self.suite)
        capture["provenance"]["classification"] = "hardware-evidence"
        with self.assertRaisesRegex(CaptureValidationError, "contradicts"):
            validate_capture_record(capture, self.suite)

    def test_accepts_explicit_diagnostic_downgrade(self) -> None:
        capture = valid_capture(self.suite)
        capture["provenance"]["classification"] = "diagnostic-only"
        validate_capture_record(capture, self.suite)

    def test_rejects_suite_artifact_hash_substitution(self) -> None:
        capture = valid_capture(self.suite)
        capture["artifact"]["adf_sha256"] = "ff" * 32
        with self.assertRaisesRegex(CaptureValidationError, "suite artifact"):
            validate_capture_record(capture, self.suite)

    def test_bound_file_checks_hash_capture_decode_config_and_artifacts(self) -> None:
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
                suite_root / "fixed-lores-sprite.adf": b"adf",
                suite_root / "fixed-lores-sprite.bin": b"payload",
            }
            for path, content in files.items():
                path.write_bytes(content)

            suite = suite_fixture()
            suite_artifact = suite["artifacts"][0]
            suite_artifact["sha256"] = {
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
            capture_path = capture_root / "capture.json"
            suite_path = suite_root / "suite-v1.json"

            validate_capture_record(capture, suite)
            verify_bound_files(capture_path, suite_path, capture)

            (capture_root / "capture.rgba").write_bytes(b"changed")
            with self.assertRaisesRegex(CaptureValidationError, "raw capture SHA-256"):
                verify_bound_files(capture_path, suite_path, capture)

    def test_bound_file_checks_reject_paths_outside_the_package(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            capture_root = root / "capture"
            suite_root = root / "suite"
            capture_root.mkdir()
            suite_root.mkdir()
            capture = valid_capture(suite_fixture())
            capture["source_capture"]["file_name"] = "../outside.rgba"

            with self.assertRaisesRegex(CaptureValidationError, "package-relative"):
                verify_bound_files(
                    capture_root / "capture.json",
                    suite_root / "suite-v1.json",
                    capture,
                )


if __name__ == "__main__":
    unittest.main()
