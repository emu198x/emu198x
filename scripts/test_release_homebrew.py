#!/usr/bin/env python3
"""Exercise the release workflow's Homebrew loop without publishing anything."""

from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/release.yml"


def publisher() -> str:
    """Read the named job; fail if its workflow shape changes."""
    return (
        WORKFLOW.read_text()
        .split("  publish-homebrew-formula:\n", 1)[1]
        .split("\n  announce:", 1)[0]
    )


def commit_script() -> str:
    block = publisher().split("      - name: Commit formula files\n", 1)[1]
    script = block.split("        run: |\n", 1)[1]
    return textwrap.dedent(script)


class HomebrewReleaseTest(unittest.TestCase):
    def run_publisher(self, plan: dict[str, object] | None) -> tuple[int, list[str]]:
        with tempfile.TemporaryDirectory(prefix="emu-homebrew-test-") as temp:
            directory = Path(temp)
            (directory / "Formula").mkdir()
            if plan is not None:
                (directory / "Formula/plan-dist-manifest.json").write_text(
                    json.dumps(plan)
                )
            # Functions keep the stubs ahead of Homebrew's own PATH changes.
            stubs = """
            git() { printf '%s\\n' "git $*" >> "$COMMAND_LOG"; }
            brew() { printf '%s\\n' "brew $*" >> "$COMMAND_LOG"; }
            """
            log = directory / "commands.log"
            result = subprocess.run(
                ["/bin/bash", "-e", "-c", stubs + commit_script()],
                cwd=directory,
                env={
                    "PATH": os.environ["PATH"],
                    "COMMAND_LOG": str(log),
                    "GITHUB_USER": "test",
                    "GITHUB_EMAIL": "test@example.invalid",
                },
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            )
            return (
                result.returncode,
                log.read_text().splitlines() if log.exists() else [],
            )

    def test_plan_is_not_exported_into_every_process(self) -> None:
        self.assertIsNone(re.search(r"^\s+PLAN:", publisher(), re.MULTILINE))

    def test_oversized_plan_publishes_only_its_formulae(self) -> None:
        plan = {
            "description": "x" * 250_000,
            "releases": [
                {"app_version": "0.32.1", "artifacts": ["spectrum.rb", "spectrum.tgz"]},
                {"app_version": "0.32.1", "artifacts": ["amiga.rb"]},
                {"app_version": "9.9.9", "artifacts": ["source.tar.gz"]},
            ],
        }
        status, commands = self.run_publisher(plan)
        self.assertEqual(status, 0)
        self.assertEqual(
            [line for line in commands if line.startswith("git add ")],
            ["git add Formula/spectrum.rb", "git add Formula/amiga.rb"],
        )
        self.assertEqual(
            [line for line in commands if line.startswith("git commit ")],
            ["git commit -m spectrum 0.32.1", "git commit -m amiga 0.32.1"],
        )
        self.assertEqual(commands.count("git push"), 1)

    def test_missing_or_empty_plan_cannot_report_publication(self) -> None:
        for plan in (None, {"releases": []}):
            with self.subTest(plan=plan):
                status, commands = self.run_publisher(plan)
                self.assertNotEqual(status, 0)
                self.assertFalse(any(line.startswith("git ") for line in commands))


if __name__ == "__main__":
    unittest.main()
