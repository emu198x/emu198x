#!/usr/bin/env python3
"""Binary-only selection must work without dropping real or failing doctests."""
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from run_doctests import doctest_packages, run_doctests


def metadata() -> dict:
    return {
        "workspace_members": ["bin", "lib", "disabled", "macro"],
        "packages": [
            {"id": "bin", "name": "app", "targets": [{"kind": ["bin"], "doctest": False}]},
            {"id": "lib", "name": "core", "targets": [{"kind": ["rlib", "cdylib"], "doctest": True}]},
            {"id": "disabled", "name": "no-docs", "targets": [{"kind": ["lib"], "doctest": False}]},
            {"id": "macro", "name": "derive", "targets": [{"kind": ["proc-macro"], "doctest": True}]},
            {"id": "external", "name": "dependency", "targets": [{"kind": ["lib"], "doctest": True}]},
        ],
    }


class SelectionTests(unittest.TestCase):
    def test_binary_only_has_no_doctest_target(self) -> None:
        self.assertEqual(doctest_packages(metadata(), ["app"]), [])

    def test_mixed_selection_keeps_only_real_library_targets(self) -> None:
        self.assertEqual(doctest_packages(metadata(), ["app", "core", "no-docs"]), ["core"])

    def test_all_mode_covers_workspace_libraries_and_proc_macros(self) -> None:
        self.assertEqual(doctest_packages(metadata(), []), ["core", "derive"])

    def test_missing_or_external_package_is_an_error(self) -> None:
        for name in ["typo", "dependency"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                doctest_packages(metadata(), [name])

    def test_empty_or_incomplete_metadata_cannot_pass_as_no_doctests(self) -> None:
        bad = metadata()
        bad["workspace_members"].append("missing")
        for data in [bad, {"workspace_members": [], "packages": []}]:
            with self.assertRaises(ValueError):
                doctest_packages(data, [])

    def test_metadata_failure_stops_before_any_test_command(self) -> None:
        with mock.patch("run_doctests.subprocess.run", side_effect=subprocess.CalledProcessError(1, "cargo metadata")) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                run_doctests(["app"])
            self.assertEqual(run.call_count, 1)

    def test_real_cargo_binary_mixed_and_failing_library(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            (root / "Cargo.toml").write_text('[workspace]\nmembers=["app","doclib"]\ndefault-members=["app"]\nresolver="2"\n')
            for name, target in [("app", "main"), ("doclib", "lib")]:
                package = root / name
                (package / "src").mkdir(parents=True)
                (package / "Cargo.toml").write_text(f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2024"\n')
                (package / "src" / f"{target}.rs").write_text("fn main() {}\n" if target == "main" else "/// ```\n/// assert_eq!(doclib::answer(), 42);\n/// ```\npub fn answer() -> u8 {42}\n")
            with mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": str(root / "target")}):
                subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root, check=True, capture_output=True)
                baseline = subprocess.run(["cargo", "test", "--doc", "--package", "app"], cwd=root, capture_output=True, text=True)
                self.assertNotEqual(baseline.returncode, 0)
                self.assertIn("no library targets", baseline.stderr)
                with (root / "doctests.log").open("w+") as log:
                    self.assertEqual(run_doctests(["app"], root, output=log), 0)
                    self.assertEqual(run_doctests(["app", "doclib"], root, output=log), 0)
                    source = root / "doclib/src/lib.rs"
                    source.write_text(source.read_text().replace("answer(), 42", "answer(), 43"))
                    self.assertNotEqual(run_doctests([], root, output=log), 0)
                    log.seek(0)
                    self.assertIn("1 passed", log.read())


if __name__ == "__main__":
    unittest.main()
