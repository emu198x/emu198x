#!/usr/bin/env python3
"""Focused tests for the Amiga closure asset-identity verifier."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("verify-amiga-closure-assets.py")
SPEC = importlib.util.spec_from_file_location("verify_amiga_closure_assets", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
assets = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(assets)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class ClosureAssetVerifierTests(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[Path, dict[str, Path]]:
        payload = b"fixture-adf-payload"
        direct_root = root / "direct"
        archive_root = root / "archive"
        direct_root.mkdir()
        archive_root.mkdir()
        (direct_root / "fixture.adf").write_bytes(payload)
        archive_path = archive_root / "fixture.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            archive.writestr("notes.txt", "not media")
            archive.writestr("Fixture.adf", payload)
        archive_bytes = archive_path.read_bytes()

        manifest = {
            "schema_version": "1.0.0",
            "scope": {
                "lanes": ["golden-matrix", "catalogue-ten"],
                "logical_asset_count": 1,
                "source_use_count": 2,
            },
            "roots": {
                "direct": {"kind": "directory"},
                "archive": {"kind": "directory"},
            },
            "assets": [
                {
                    "id": "fixture-disk",
                    "kind": "disk",
                    "payload": {"bytes": len(payload), "sha256": digest(payload)},
                    "uses": [
                        {
                            "lane": "golden-matrix",
                            "consumers": ["fixture-golden"],
                            "root": "direct",
                            "relative_path": "fixture.adf",
                            "source": {
                                "bytes": len(payload),
                                "sha256": digest(payload),
                            },
                        },
                        {
                            "lane": "catalogue-ten",
                            "consumers": ["fixture-catalogue"],
                            "root": "archive",
                            "relative_path": "fixture.zip",
                            "archive_member": "Fixture.adf",
                            "source": {
                                "bytes": len(archive_bytes),
                                "sha256": digest(archive_bytes),
                            },
                        },
                    ],
                }
            ],
        }
        manifest_path = root / "manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        return manifest_path, {"direct": direct_root, "archive": archive_root}

    def test_direct_and_archived_payloads_verify_without_reporting_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, roots = self.fixture(root)

            report = assets.verify_manifest(manifest, roots)

            self.assertEqual(report["status"], "pass")
            self.assertEqual(report["logical_asset_count"], 1)
            self.assertEqual(report["source_use_count"], 2)
            self.assertNotIn(str(root), json.dumps(report))

    def test_source_change_fails_before_payload_can_be_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, roots = self.fixture(root)
            (roots["direct"] / "fixture.adf").write_bytes(b"changed")

            with self.assertRaisesRegex(
                assets.VerificationError, "source byte count mismatch"
            ):
                assets.verify_manifest(manifest, roots)

    def test_golden_subset_requires_only_its_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, roots = self.fixture(root)

            report = assets.verify_manifest(
                manifest,
                {"direct": roots["direct"]},
                ("golden-matrix",),
            )

            self.assertEqual(report["lanes"], ["golden-matrix"])
            self.assertEqual(report["logical_asset_count"], 1)
            self.assertEqual(report["source_use_count"], 1)

    def test_catalogue_subset_requires_only_its_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, roots = self.fixture(root)

            report = assets.verify_manifest(
                manifest,
                {"archive": roots["archive"]},
                ("catalogue-ten",),
            )

            self.assertEqual(report["lanes"], ["catalogue-ten"])
            self.assertEqual(report["logical_asset_count"], 1)
            self.assertEqual(report["source_use_count"], 1)

    def test_archive_member_must_match_loader_selection(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path, roots = self.fixture(root)
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["assets"][0]["uses"][1]["archive_member"] = "Other.adf"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            with self.assertRaisesRegex(
                assets.VerificationError,
                "selected archive member does not match manifest",
            ):
                assets.verify_manifest(manifest_path, roots)

    def test_registered_manifest_contains_no_absolute_or_operator_paths(self) -> None:
        manifest_path = (
            Path(__file__).resolve().parent.parent
            / "test-data/commodore/amiga/closure-assets-v1.json"
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        serialized = json.dumps(manifest)

        self.assertNotIn("/Users/", serialized)
        self.assertNotIn("/Volumes/", serialized)
        self.assertNotIn("/private/", serialized)
        self.assertNotIn("stevehill", serialized.lower())
        validated = assets.validate_manifest(manifest)
        self.assertEqual(len(validated), 16)


if __name__ == "__main__":
    unittest.main()
