"""The rule-34 registry gate must reject hidden machine dependencies too."""

import pathlib
import tempfile
import unittest

from check_registry import machine_dependency_problems, moved_board_problems


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

    def test_published_names_keep_their_category(self) -> None:
        # Publishing adds `emu198x-` and keeps the category word
        # (knowledge/decisions/crate-naming.md), on either side of the edge.
        failures = self.check(
            '[dependencies]\nemu198x-machine-other = { path = "../other" }\n'
        )
        self.assertEqual(len(failures), 1)
        self.assertIn("emu198x-machine-other", failures[0])
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            machine = root / "crates" / "emu198x-machine-example"
            machine.mkdir(parents=True)
            (machine / "Cargo.toml").write_text(
                '[package]\nname = "emu198x-machine-example"\n'
                '[dependencies]\nmachine-other = { path = "../other" }\n'
            )
            failures = machine_dependency_problems(root)
        self.assertEqual(len(failures), 1)
        self.assertIn("emu198x-machine-example", failures[0])

    def test_empty_scan_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            self.assertEqual(len(machine_dependency_problems(root)), 1)


REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


class MovedBoardTest(unittest.TestCase):
    """A board in another repository must be a published dependency of its binary."""

    system = {
        "machine_id": "cv-ntsc",
        "crate": "emu198x-colecovision",
        "machine_repository": "emu198x/coleco-colecovision",
    }
    board = "emu198x-machine-coleco-colecovision-ntsc"

    def lock(self, board_source: str = REGISTRY, reached: bool = True) -> dict:
        return {"package": [
            {"name": "emu198x-colecovision",
             "dependencies": ["emu198x-runtime-coleco-colecovision"] if reached else []},
            {"name": "emu198x-runtime-coleco-colecovision", "source": REGISTRY,
             "dependencies": [self.board]},
            {"name": self.board, "source": board_source},
        ]}

    def test_a_published_board_its_binary_reaches_passes(self) -> None:
        self.assertEqual(moved_board_problems(self.system, self.board, self.lock()), [])

    def test_a_board_missing_from_the_lock_fails(self) -> None:
        problems = moved_board_problems(self.system, "emu198x-machine-absent", self.lock())
        self.assertEqual(len(problems), 1)
        self.assertIn("not in Cargo.lock", problems[0])

    def test_a_board_not_from_crates_io_fails(self) -> None:
        problems = moved_board_problems(self.system, self.board, self.lock(board_source=""))
        self.assertTrue(any("not from crates.io" in p for p in problems))

    def test_a_board_the_binary_does_not_reach_fails(self) -> None:
        problems = moved_board_problems(self.system, self.board, self.lock(reached=False))
        self.assertTrue(any("does not depend on" in p for p in problems))

    def test_the_repository_must_be_an_emu198x_one(self) -> None:
        system = {**self.system, "machine_repository": "someone/else"}
        problems = moved_board_problems(system, self.board, self.lock())
        self.assertTrue(any("not an emu198x repository" in p for p in problems))


if __name__ == "__main__":
    unittest.main()
