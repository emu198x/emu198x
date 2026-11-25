#!/usr/bin/env python3
"""Check extract-repo.py's manifest rewriting and its repository list."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import pathlib
import re
import tempfile
import textwrap
import tomllib
import unittest
import unittest.mock
import urllib.error

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
        with tempfile.TemporaryDirectory() as tmp, tempfile.TemporaryDirectory() as workspace:
            # A stand-in for this workspace, so the test does not depend on
            # which crates have not left it yet.
            for name in ("emu198x-atari-gtia", "emu198x-mos-6502"):
                crate = pathlib.Path(workspace) / "crates" / name
                crate.mkdir(parents=True)
                (crate / "Cargo.toml").write_text("")
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
            with unittest.mock.patch.object(extract_repo, "ROOT", pathlib.Path(workspace)):
                siblings = extract_repo.unreleased_siblings(pathlib.Path(tmp))
            self.assertEqual(siblings, {"emu198x-atari-gtia", "emu198x-mos-6502"})


class VerificationMode(unittest.TestCase):
    def check(self, registry_has_helper: bool) -> tuple[str, list[tuple[str, ...]]]:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            crate = root / "crates/emu198x-test-skip"
            crate.mkdir(parents=True)
            (crate / "Cargo.toml").write_text('[package]\nversion = "0.1.0"\n')
            output = io.StringIO()
            with (
                contextlib.redirect_stdout(output),
                unittest.mock.patch.object(extract_repo, "ROOT", root),
                unittest.mock.patch.object(extract_repo, "published", return_value=registry_has_helper),
                unittest.mock.patch.object(extract_repo, "run") as run,
            ):
                extract_repo.verify(root / "extracted")
            return output.getvalue(), [call.args for call in run.call_args_list]

    def test_local_checks_are_labelled_and_skip_publication_dry_run(self) -> None:
        output, commands = self.check(False)
        self.assertIn("emu198x-test-skip; not registry-only", output)
        self.assertEqual([args[1] for args in commands], ["fmt", "clippy", "test"])
        self.assertIn("--config", commands[-1])

    def test_checks_without_local_overrides_attempt_publication_dry_run(self) -> None:
        output, commands = self.check(True)
        self.assertIn("no temporary path overrides", output)
        self.assertEqual(commands[-1], ("cargo", "publish", "--dry-run", "--workspace"))


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


class WorkspaceDependencies(unittest.TestCase):
    def test_a_crate_left_behind_becomes_a_registry_version(self) -> None:
        # A system family inherits emu198x-shell from this workspace, where
        # it is a path that won't exist in the family's repository.
        self.assertEqual(
            extract_repo.registry_spec(
                "emu198x-shell", '{ path = "crates/emu198x-shell", version = "0.44.2" }'
            ),
            '"0.44.2"',
        )
        self.assertEqual(
            extract_repo.registry_spec(
                "emu198x-ui",
                '{ path = "crates/emu198x-ui", version = "0.46.2", default-features = false }',
            ),
            '{ version = "0.46.2", default-features = false }',
        )

    def test_a_registry_dependency_is_left_alone(self) -> None:
        for spec in ('"1"', '{ version = "1", features = ["derive"] }'):
            self.assertEqual(extract_repo.registry_spec("serde", spec), spec)

    def test_an_inherited_path_reports_its_exact_registry_version_without_rewriting_it(self) -> None:
        specs = {
            "shell": '{ path = "crates/emu198x-shell", package = "emu198x-shell", version = "0.44.4" }',
            "unused": '{ path = "crates/unused", version = "0.1.0" }',
            "serde": '"1"',
            "range": '{ path = "crates/range", version = ">=0.1, <0.2" }',
        }
        with unittest.mock.patch.object(extract_repo, "published", return_value=False) as published:
            warnings = extract_repo.inherited_registry_warnings(specs, {"shell", "serde", "range"})
        published.assert_called_once_with("emu198x-shell", "0.44.4")
        self.assertEqual(warnings, [
            "workspace dependency shell (emu198x-shell): exact crates.io version "
            "0.44.4 was not found; Cargo may still resolve a compatible version"
        ])
        self.assertEqual(
            extract_repo.registry_spec("shell", specs["shell"]),
            '{ package = "emu198x-shell", version = "0.44.4" }',
        )

    def test_a_registry_outage_does_not_mean_the_inherited_version_is_missing(self) -> None:
        spec = '{ path = "crates/emu198x-shell", version = "0.44.4" }'
        error = urllib.error.HTTPError("https://crates.io", 503, "unavailable", {}, None)
        with unittest.mock.patch.object(extract_repo.urllib.request, "urlopen", side_effect=error):
            warnings = extract_repo.inherited_registry_warnings(
                {"emu198x-shell": spec}, {"emu198x-shell"},
            )
        self.assertEqual(warnings, [
            "workspace dependency emu198x-shell: could not check crates.io for emu198x-shell 0.44.4"
        ])

    def test_a_path_without_a_version_cannot_move(self) -> None:
        with self.assertRaises(SystemExit):
            extract_repo.registry_spec("x", '{ path = "crates/x" }')


class GeneratedCi(unittest.TestCase):
    def ci(self, deps: set[str]) -> str:
        with tempfile.TemporaryDirectory() as tmp, unittest.mock.patch.object(
            extract_repo,
            "workspace_dependencies",
            return_value={"emu198x-shell": '"0.44.2"', "serde": '"1"'},
        ):
            target = pathlib.Path(tmp)
            extract_repo.skeleton(
                {"name": "test-family", "summary": "fixture", "coverage_floor": 70},
                target,
                ["test-crate"],
                deps,
            )
            return (target / ".github/workflows/ci.yml").read_text()

    def test_a_repository_on_the_shell_installs_its_linux_packages(self) -> None:
        # The ColecoVision's first CI run failed to build alsa-sys and
        # libudev-sys without these.
        self.assertIn(
            "linux-packages: pkg-config libasound2-dev libudev-dev",
            self.ci({"emu198x-shell", "serde"}),
        )

    def test_a_chip_without_the_shell_installs_nothing(self) -> None:
        self.assertNotIn("linux-packages", self.ci({"serde"}))


class GeneratedGuidance(unittest.TestCase):
    def test_extracted_guidance_requires_accuracy_without_banning_bus_shapes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = pathlib.Path(tmp)
            extract_repo.skeleton(
                {"name": "test-extracted-cpu", "summary": "CPU fixture"},
                target,
                ["test-cpu"],
                set(),
            )
            guidance = (target / "AGENTS.md").read_text()
        self.assertNotRegex(guidance, r"(?i)no (?:bus trait|bus callback|method-call)")
        self.assertIn("cycle-accurate", guidance)
        self.assertIn("hardware signals and timing", guidance)


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
