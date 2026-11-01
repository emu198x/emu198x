"""The rule-34 registry gate must reject hidden machine dependencies too."""

import pathlib
import tempfile
import unittest

from check_registry import machine_dependency_problems


class MachineDependenciesTest(unittest.TestCase):
    def check(self, dependency: str, workspace: str = "") -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n" + workspace)
            machine = root / "crates" / "machine-example"
            machine.mkdir(parents=True)
            (machine / "Cargo.toml").write_text(
                '[package]\nname = "machine-example"\n' + dependency
            )
            return machine_dependency_problems(root)

    def test_shared_common_hardware_is_allowed(self) -> None:
        self.assertEqual(self.check('[dependencies]\ncommon-example = "1"\n'), [])

    def test_original_amiga_edge_is_rejected(self) -> None:
        failures = self.check(
            '[dependencies]\nmachine-commodore-amiga-ocs = { path = "../ocs" }\n'
        )
        self.assertEqual(len(failures), 1)
        self.assertIn("machine-commodore-amiga-ocs", failures[0])

    def test_target_renamed_inherited_dependency_is_rejected(self) -> None:
        for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
            with self.subTest(kind=kind):
                failures = self.check(
                    f"[target.'cfg(unix)'.{kind}]\nboard = {{ workspace = true }}\n",
                    "[workspace.dependencies]\n"
                    'board = { package = "machine-other", path = "crates/other" }\n',
                )
                self.assertEqual(len(failures), 1)
                self.assertIn("machine-other", failures[0])
                self.assertIn(kind, failures[0])

    def test_empty_scan_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            self.assertEqual(len(machine_dependency_problems(root)), 1)


if __name__ == "__main__":
    unittest.main()
