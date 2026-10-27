#!/usr/bin/env python3
"""Check extract-repo.py's manifest rewriting and its repository list."""

from __future__ import annotations

import importlib.util
import pathlib
import re
import tempfile
import textwrap
import tomllib
import unittest
import unittest.mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
CRATES_IO_NAME_LIMIT = 64
SPEC = importlib.util.spec_from_file_location("extract_repo", ROOT / "scripts" / "extract-repo.py")
extract_repo = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extract_repo)


def rewrite(manifest: str, moving: set[str]) -> tuple[str, set[str], list[str]]:
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "crate" / "Cargo.toml"
        path.parent.mkdir()
        path.write_text(textwrap.dedent(manifest))
        used: set[str] = set()
        with unittest.mock.patch.object(extract_repo, "published", return_value=True):
            warnings = extract_repo.rewrite_manifest(path, moving, used)
        return path.read_text(), used, warnings


class UnreleasedSiblings(unittest.TestCase):
    def test_a_registry_dependency_still_in_this_workspace_is_a_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            manifest = pathlib.Path(tmp) / "crates" / "emu198x-atari-antic" / "Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(textwrap.dedent(
                """\
                [dependencies]
                emu198x-atari-gtia = "0.1.0"
                emu198x-mos-6502 = { version = "0.6.0", features = [] }
                serde = { workspace = true }
                not-a-crate-here = "1"
                """
            ))
            self.assertEqual(
                extract_repo.unreleased_siblings(pathlib.Path(tmp)),
                {"emu198x-atari-gtia", "emu198x-mos-6502"},
            )


class RewriteManifest(unittest.TestCase):
    def test_a_dependency_left_behind_comes_from_the_registry(self) -> None:
        text, _, _ = rewrite(
            """\
            [package]
            name = "a"
            version = "0.1.0"

            [dependencies]
            b = { path = "../b", version = "0.3.0" }
            c = { path = "../c", version = "0.2.0", features = ["x"] }
            """,
            moving={"a"},
        )
        self.assertIn('b = "0.3.0"\n', text)
        self.assertIn('c = { version = "0.2.0", features = ["x"] }\n', text)
        self.assertNotIn("path", text)

    def test_a_dependency_moving_with_it_stays_a_path(self) -> None:
        text, _, _ = rewrite(
            """\
            [package]
            name = "a"
            version = "0.1.0"

            [dependencies]
            b = { path = "../b", version = "0.3.0" }
            """,
            moving={"a", "b"},
        )
        self.assertIn('b = { path = "../b", version = "0.3.0" }', text)

    def test_the_test_helper_takes_its_registry_requirement_and_loses_its_comment(self) -> None:
        text, used, _ = rewrite(
            """\
            [package]
            name = "a"
            version = "0.1.0"

            [dev-dependencies]
            # Path-only on purpose, while the helper is unpublished.
            emu198x-test-skip = { path = "../emu198x-test-skip" }
            serde_json = { workspace = true }
            """,
            moving={"a"},
        )
        self.assertIn('[dev-dependencies]\nemu198x-test-skip = "0.1"\n', text)
        self.assertNotIn("unpublished", text)
        self.assertEqual(used, {"serde_json"})

    def test_an_unpublished_version_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "a" / "Cargo.toml"
            path.parent.mkdir()
            path.write_text('[package]\nversion = "0.1.0"\n\n[dependencies]\nb = { path = "../b", version = "9.9.9" }\n')
            with unittest.mock.patch.object(extract_repo, "published", return_value=False):
                warnings = extract_repo.rewrite_manifest(path, {"a"}, set())
        self.assertEqual(warnings, ["a: b 9.9.9 is not on crates.io yet"])

    def test_a_suite_versioned_crate_cannot_move(self) -> None:
        with self.assertRaises(SystemExit):
            rewrite('[package]\nname = "a"\nversion.workspace = true\n', moving={"a"})

    def test_an_unpublishable_crate_cannot_move(self) -> None:
        with self.assertRaises(SystemExit):
            rewrite('[package]\nname = "a"\nversion = "0.1.0"\npublish = false\n', moving={"a"})


class RepositoryList(unittest.TestCase):
    """Every listed crate must be able to move today."""

    repos = tomllib.loads(extract_repo.CONFIG.read_text())["repo"]

    def test_names_are_unique(self) -> None:
        names = [repo["name"] for repo in self.repos]
        self.assertEqual(len(names), len(set(names)))

    def test_every_crate_is_published_prefixed_and_independently_versioned(self) -> None:
        for repo in self.repos:
            for crate in repo["crates"]:
                with self.subTest(crate=crate):
                    manifest = (ROOT / "crates" / crate / "Cargo.toml").read_text()
                    self.assertTrue(crate.startswith("emu198x-"))
                    # crates.io rejects longer names; see crate-naming.md.
                    self.assertLessEqual(len(crate), CRATES_IO_NAME_LIMIT)
                    self.assertRegex(manifest, r'(?m)^version = "')
                    self.assertNotRegex(manifest, r"(?m)^publish\s*=\s*false")

    def test_every_carried_data_path_exists(self) -> None:
        for repo in self.repos:
            for data in repo.get("data", []):
                with self.subTest(data=data):
                    self.assertTrue((ROOT / data).exists())

    def test_every_corpus_job_names_a_test_its_crate_has(self) -> None:
        for repo in self.repos:
            for corpus in repo.get("corpus", []):
                with self.subTest(corpus=corpus["artifact"]):
                    self.assertIn(corpus["crate"], repo["crates"])
                    test = ROOT / "crates" / corpus["crate"] / "tests" / f"{corpus['test']}.rs"
                    self.assertTrue(test.exists(), test)
                    if corpus.get("filter"):
                        self.assertRegex(test.read_text(), rf"fn {re.escape(corpus['filter'])}\b")


if __name__ == "__main__":
    unittest.main()
