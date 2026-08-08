#!/usr/bin/env python3
"""Focused tests for verify-c64-vicii-survey.py."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("verify-c64-vicii-survey.py")
SPEC = importlib.util.spec_from_file_location("verify_c64_vicii_survey", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import {SCRIPT}")
survey = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = survey
SPEC.loader.exec_module(survey)


class VerifyC64ViciiSurveyTests(unittest.TestCase):
    def fixture(
        self, root: Path
    ) -> tuple[Path, Path, Path, dict[str, object]]:
        testbench = root / "private-testbench"
        roms = root / "private-roms"
        assets: list[dict[str, object]] = []
        for asset_id, role, root_id, relative in survey.expected_asset_contract():
            asset_root = testbench if root_id == "testbench" else roms
            path = asset_root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            payload = f"fixture bytes for {asset_id}\n".encode()
            path.write_bytes(payload)
            assets.append(
                {
                    "id": asset_id,
                    "role": role,
                    "root": root_id,
                    "relative_path": relative,
                    "bytes": len(payload),
                    "sha256": survey.sha256_bytes(payload),
                }
            )

        manifest = {
            "schema": survey.ASSET_SCHEMA,
            "id": survey.ASSET_MANIFEST_ID,
            "source": {
                "suite": "VICE VIC-II testbench",
                "holding": "test fixture",
                "upstream_revision": "unresolved",
            },
            "scope": {
                "model": "6569",
                "case_ids": [case_id for case_id, _, _ in survey.EXPECTED_CASES],
                "asset_count": len(assets),
            },
            "assets": assets,
        }
        manifest_path = root / "assets-v1.json"
        manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        return manifest_path, testbench, roms, manifest

    @staticmethod
    def producer(revision: str = "a" * 40, dirty: bool = False) -> dict[str, object]:
        cases = [
            {
                "id": case_id,
                "program": program,
                "reference": reference,
                "reference_width": 384,
                "reference_height": 272,
                "reference_color_type": "rgba8",
                "reference_indexed_sha256": "b" * 64,
                "actual_indexed_sha256": "c" * 64,
                "matched_pixels": 100_000,
                "total_pixels": 384 * 272,
            }
            for case_id, program, reference in survey.EXPECTED_CASES
        ]
        return {
            "schema": survey.PRODUCER_SCHEMA,
            "revision": revision,
            "dirty": dirty,
            "runtime_contract": {
                "model": "c64-pal-breadbin",
                "vic_model": "6569",
                "boot_frames": 150,
                "settle_frames": 60,
                "framebuffer_width": 416,
                "framebuffer_height": 312,
                "typed_command": "RUN\n",
            },
            "comparison_contract": {
                "method": "nearest-c64-palette-index-squared-rgb-v1",
                "crop_x": 16,
                "crop_y": 16,
                "reference_width": 384,
                "reference_height": 272,
                "assertion_boundary": "digital-colour-index-output-not-analogue-colour",
            },
            "cases": cases,
        }

    def test_asset_manifest_verifies_exact_order_hashes_and_counts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path, testbench, roms, _ = self.fixture(root)
            result = survey.load_and_verify_assets(manifest_path, testbench, roms)

            self.assertEqual(result["verified_asset_count"], 29)
            self.assertEqual(result["scope"]["asset_count"], 29)
            self.assertEqual(
                [asset["id"] for asset in result["assets"]],
                [contract[0] for contract in survey.expected_asset_contract()],
            )

            first = testbench / survey.EXPECTED_CASES[0][1]
            first.write_bytes(b"changed")
            with self.assertRaisesRegex(
                survey.VerificationError, "does not match its registered identity"
            ):
                survey.load_and_verify_assets(manifest_path, testbench, roms)

    def test_asset_manifest_rejects_a_reordered_contract(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path, testbench, roms, manifest = self.fixture(root)
            manifest["assets"][0], manifest["assets"][1] = (
                manifest["assets"][1],
                manifest["assets"][0],
            )
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(
                survey.VerificationError, "not the expected program:gfxfetch contract"
            ):
                survey.load_and_verify_assets(manifest_path, testbench, roms)

    def test_producer_validation_preserves_integer_measurement_contract(self) -> None:
        revision = "d" * 40
        result = survey.validate_producer(self.producer(revision), revision, False)
        cases = result["cases"]

        self.assertEqual(len(cases), 13)
        self.assertEqual(cases[0]["id"], "gfxfetch")
        self.assertEqual(cases[0]["matched_pixels"], 100_000)
        self.assertEqual(cases[0]["total_pixels"], 104_448)
        self.assertEqual(cases[0]["status"], "measured")
        self.assertEqual(cases[0]["match_percent"], 95.741)

    def test_producer_validation_rejects_wrong_identity_order_and_dimensions(self) -> None:
        revision = "e" * 40

        wrong_revision = self.producer("f" * 40)
        with self.assertRaisesRegex(survey.VerificationError, "repository identity"):
            survey.validate_producer(wrong_revision, revision, False)

        reordered = self.producer(revision)
        reordered["cases"][0], reordered["cases"][1] = (
            reordered["cases"][1],
            reordered["cases"][0],
        )
        with self.assertRaisesRegex(survey.VerificationError, "differs from gfxfetch"):
            survey.validate_producer(reordered, revision, False)

        wrong_dimensions = self.producer(revision)
        wrong_dimensions["cases"][0]["reference_width"] = 383
        with self.assertRaisesRegex(survey.VerificationError, "reference format"):
            survey.validate_producer(wrong_dimensions, revision, False)

    def test_unsafe_paths_and_duplicate_json_keys_are_rejected(self) -> None:
        for path in ("../reference.png", "/private/reference.png", "a/../b.png"):
            with self.subTest(path=path):
                with self.assertRaises(survey.VerificationError):
                    survey.assert_safe_relative_path(path, "fixture")

        with self.assertRaisesRegex(survey.VerificationError, "repeats key"):
            survey.decode_json_bytes(b'{"schema":"one","schema":"two"}', "fixture")

    def test_dirty_policy_and_report_directory_are_explicit(self) -> None:
        revision = "1" * 40
        survey.require_dirty_policy(False, False)
        survey.require_dirty_policy(True, True)
        with self.assertRaisesRegex(survey.VerificationError, "clean worktree"):
            survey.require_dirty_policy(True, False)
        self.assertEqual(survey.report_directory_name(revision, False), revision)
        self.assertEqual(
            survey.report_directory_name(revision, True), f"{revision}-dirty"
        )

    def test_atomic_json_write_replaces_a_complete_document(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report.json"
            survey.atomic_write_json(report, {"status": "running", "cases": []})
            survey.atomic_write_json(report, {"status": "complete", "cases": [1]})
            self.assertEqual(
                json.loads(report.read_text(encoding="utf-8")),
                {"status": "complete", "cases": [1]},
            )
            self.assertEqual(list(report.parent.glob(".report.json.*.tmp")), [])

    def test_assembled_report_contains_logical_ids_but_no_host_paths(self) -> None:
        revision = "2" * 40
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path, testbench, roms, _ = self.fixture(root)
            fixtures = survey.load_and_verify_assets(manifest_path, testbench, roms)
            measured = survey.validate_producer(
                self.producer(revision), revision, False
            )
            report = survey.base_report(
                revision,
                False,
                "2026-08-09T00:00:00.000Z",
                fixtures,
            )
            report["status"] = "complete"
            report["runtime_contract"] = measured["runtime_contract"]
            report["comparison_contract"] = measured["comparison_contract"]
            report["cases"] = measured["cases"]

            encoded = json.dumps(report)
            self.assertNotIn(str(root), encoded)
            self.assertNotIn(str(testbench), encoded)
            self.assertNotIn(str(roms), encoded)
            self.assertIn("program:gfxfetch", encoded)
            self.assertIn("gfxfetch/gfxfetch.prg", encoded)


if __name__ == "__main__":
    unittest.main()
