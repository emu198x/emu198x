#!/usr/bin/env python3
"""Focused tests for retained FS-UAE APNG remeasurement."""

from __future__ import annotations

import importlib.util
import json
import unittest
from pathlib import Path


TOOLS_DIR = Path(__file__).resolve().parent
SCRIPT = TOOLS_DIR / "verify_fs_uae_package.py"
PACKAGE_ROOT = TOOLS_DIR.parent / "references/fs-uae-5.0.7-f362278c"
SPEC = importlib.util.spec_from_file_location("verify_fs_uae_package", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


class FsUaePackageVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.report = verifier.verify_package(PACKAGE_ROOT)

    def test_registered_package_remeasures_all_retained_frames(self) -> None:
        self.assertEqual(self.report["status"], "pass")
        self.assertEqual(self.report["run_count"], 10)
        self.assertEqual(self.report["frame_count"], 30)
        self.assertTrue(self.report["all_adjacent_frames_byte_identical"])
        self.assertEqual(
            self.report["package_manifest_sha256"],
            "b6a04ae162aaf4b137a21e57c2f0ab0e5cd14bd91a4713d2b153ba1e0c95e0f3",
        )
        self.assertEqual(
            {(run["profile"], run["case_id"]) for run in self.report["runs"]},
            {
                (profile, case_id)
                for profile in verifier.PROFILES
                for case_id in verifier.CASES
            },
        )

    def test_attestation_contains_no_local_package_path(self) -> None:
        serialized = json.dumps(self.report)
        self.assertNotIn(str(PACKAGE_ROOT), serialized)
        self.assertNotIn("/Users/", serialized)
        self.assertNotIn("/private/", serialized)

    def test_semantic_remeasurement_has_no_pixel_tolerance(self) -> None:
        capture = PACKAGE_ROOT / "captures/ecs--midline-hbstrt-past.apng"
        frame = bytearray(verifier.decode_apng(capture, "fixture")[0])
        for row in (204, 205):
            offset = (row * verifier.WIDTH + 300) * 4
            frame[offset : offset + 3] = b"\x01\x02\x03"

        with self.assertRaisesRegex(
            verifier.VerificationError, "unexpected RGB at semantic sample"
        ):
            verifier.derive_semantics(
                bytes(frame), "0x0f0", "0xf0f", "fixture"
            )

    def test_package_paths_cannot_escape_the_registered_root(self) -> None:
        with self.assertRaisesRegex(verifier.VerificationError, "escapes package"):
            verifier.checked_package_path(PACKAGE_ROOT, "../outside", "fixture")


if __name__ == "__main__":
    unittest.main()
