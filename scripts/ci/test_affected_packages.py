#!/usr/bin/env python3
"""The PR test selection must never skip a crate a change can reach."""

from __future__ import annotations

import subprocess
import unittest

from affected_packages import (
    affected,
    changed_files,
    lock_changes,
    manifest_changes,
    workspace,
)

DIRS = {
    "cpu": "crates/cpu",
    "machine": "crates/machine",
    "app": "crates/app",
    "helper": "crates/helper",
    "lonely": "crates/lonely",
}
# app -> machine -> cpu, and machine's tests use helper (a dev-dependency).
DEPENDENTS = {
    "cpu": {"machine"},
    "machine": {"app"},
    "app": set(),
    "helper": {"machine"},
    "lonely": set(),
}


class AffectedTest(unittest.TestCase):
    def check(self, files: list[str]) -> dict:
        return affected(files, DIRS, DEPENDENTS)

    def test_a_crate_change_reaches_every_dependent(self) -> None:
        result = self.check(["crates/cpu/src/lib.rs"])
        self.assertEqual(result["mode"], "some")
        self.assertEqual(result["packages"], ["app", "cpu", "machine"])

    def test_a_dev_dependency_change_reaches_the_crate_whose_tests_use_it(self) -> None:
        self.assertEqual(
            self.check(["crates/helper/src/lib.rs"])["packages"],
            ["app", "helper", "machine"],
        )

    def test_unrelated_crates_are_left_out(self) -> None:
        self.assertEqual(self.check(["crates/lonely/tests/t.rs"])["packages"], ["lonely"])

    def test_prose_runs_nothing(self) -> None:
        for file in ("knowledge/decisions/x.md", "docs/plans/p.md", "plans/q.md", "README.md"):
            with self.subTest(file=file):
                self.assertEqual(self.check([file])["mode"], "none")

    def test_files_tests_read_by_path_run_everything(self) -> None:
        for file in (
            "test-data/amstrad-cpc/save-v5/x.state",
            "Cargo.lock",
            "Cargo.toml",
            ".github/workflows/ci.yml",
            "scripts/status/collect_evidence.py",
            "rust-toolchain.toml",
            "crates/README.md",
        ):
            with self.subTest(file=file):
                self.assertEqual(self.check([file])["mode"], "all")

    def test_one_file_outside_a_crate_runs_everything(self) -> None:
        result = self.check(["crates/lonely/src/lib.rs", "test-data/x.bin"])
        self.assertEqual(result["mode"], "all")

    def test_a_prefix_name_is_not_a_parent(self) -> None:
        dirs = {**DIRS, "cpu-extra": "crates/cpu-extra"}
        dependents = {**DEPENDENTS, "cpu-extra": set()}
        self.assertEqual(
            affected(["crates/cpu-extra/src/lib.rs"], dirs, dependents)["packages"],
            ["cpu-extra"],
        )


LOCK = """
[[package]]
name = "serde"
version = "1.0.0"

[[package]]
name = "bus"
version = "0.1.0"
dependencies = ["serde"]

[[package]]
name = "cpu"
version = "0.1.0"
dependencies = ["bus"]

[[package]]
name = "machine"
version = "0.1.0"
dependencies = ["cpu", "helper"]

[[package]]
name = "app"
version = "0.1.0"
dependencies = ["machine"]

[[package]]
name = "helper"
version = "0.1.0"

[[package]]
name = "lonely"
version = "0.1.0"
"""


class LockTest(unittest.TestCase):
    def test_an_external_bump_reaches_its_users_and_no_one_else(self) -> None:
        after = LOCK.replace('name = "bus"\nversion = "0.1.0"', 'name = "bus"\nversion = "0.1.1"')
        reached = lock_changes(LOCK, after)
        result = affected(["Cargo.lock"], DIRS, DEPENDENTS, reached)
        self.assertEqual(result["packages"], ["app", "cpu", "machine"])

    def test_a_deep_change_reaches_through_every_level(self) -> None:
        after = LOCK.replace('name = "serde"\nversion = "1.0.0"', 'name = "serde"\nversion = "1.0.1"')
        self.assertEqual(
            affected(["Cargo.lock"], DIRS, DEPENDENTS, lock_changes(LOCK, after))["packages"],
            ["app", "cpu", "machine"],
        )

    def test_a_new_dependency_reaches_the_crate_that_added_it(self) -> None:
        after = LOCK.replace(
            'name = "lonely"\nversion = "0.1.0"\n',
            'name = "lonely"\nversion = "0.1.0"\ndependencies = ["rand"]\n\n[[package]]\nname = "rand"\nversion = "0.9.0"\n',
        )
        self.assertEqual(
            affected(["Cargo.lock"], DIRS, DEPENDENTS, lock_changes(LOCK, after))["packages"],
            ["lonely"],
        )

    def test_an_unchanged_lock_reaches_nothing(self) -> None:
        self.assertEqual(lock_changes(LOCK, LOCK), set())

    def test_without_lock_information_a_lock_change_runs_everything(self) -> None:
        self.assertEqual(affected(["Cargo.lock"], DIRS, DEPENDENTS)["mode"], "all")


ROOT = """
[workspace]
members = ["crates/cpu"]

[workspace.package]
version = "0.46.1"

[workspace.dependencies]
serde = "1.0"
bus = "0.1.0"

[profile.dev]
opt-level = 1
"""


class ManifestTest(unittest.TestCase):
    def test_a_workspace_dependency_reaches_its_users(self) -> None:
        after = ROOT.replace('bus = "0.1.0"', 'bus = "0.1.1"')
        changed = manifest_changes(ROOT, after)
        self.assertEqual(changed, {"bus"})
        dependents = {**DEPENDENTS, "bus": {"cpu"}}
        self.assertEqual(
            affected(["Cargo.toml"], DIRS, dependents, manifest=changed)["packages"],
            ["app", "cpu", "machine"],
        )

    def test_a_new_member_adds_nothing_its_directory_does_not(self) -> None:
        after = ROOT.replace('["crates/cpu"]', '["crates/cpu", "crates/lonely"]')
        changed = manifest_changes(ROOT, after)
        self.assertEqual(changed, set())
        self.assertEqual(
            affected(["Cargo.toml", "crates/lonely/src/lib.rs"], DIRS, DEPENDENTS, manifest=changed)["packages"],
            ["lonely"],
        )

    def test_anything_else_in_the_root_manifest_runs_everything(self) -> None:
        for after in (
            ROOT.replace("opt-level = 1", "opt-level = 2"),
            ROOT.replace('version = "0.46.1"', 'version = "0.46.2"'),
            ROOT + "\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n",
        ):
            with self.subTest(after=after[-40:]):
                self.assertIsNone(manifest_changes(ROOT, after))
                self.assertEqual(
                    affected(["Cargo.toml"], DIRS, DEPENDENTS, manifest=None)["mode"],
                    "all",
                )


class RealWorkspaceTest(unittest.TestCase):
    """Against this repository's own graph, not a fixture."""

    def test_every_member_is_reachable_from_itself(self) -> None:
        dirs, dependents = workspace()
        self.assertGreater(len(dirs), 100)
        for name, directory in dirs.items():
            result = affected([f"{directory}/src/lib.rs"], dirs, dependents)
            self.assertIn(name, result["packages"], name)

    def test_a_shared_crate_change_reaches_the_machines_built_on_it(self) -> None:
        dirs, dependents = workspace()
        result = affected([f"{dirs['emu198x-shell']}/src/lib.rs"], dirs, dependents)
        for machine in ("emu198x-spectrum", "emu198x-amstrad-cpc", "emu198x-msx"):
            self.assertIn(machine, result["packages"])

    def test_the_registry_reaches_the_crate_whose_test_reads_it(self) -> None:
        dirs, dependents = workspace()
        result = affected(["docs/status/systems.toml"], dirs, dependents)
        self.assertEqual(result["mode"], "some")
        self.assertIn("emu198x-fleet-web", result["packages"])

    def test_diff_reads_the_merge_base(self) -> None:
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
        self.assertEqual(changed_files(head, head), [])


if __name__ == "__main__":
    unittest.main()
