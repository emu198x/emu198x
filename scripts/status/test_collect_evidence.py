#!/usr/bin/env python3
"""ROM-free regression checks for the workspace test gate itself."""

from __future__ import annotations

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

import collect_evidence as collector


class CollectorTest(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="emu-collector-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def executable(self, name: str, source: str) -> Path:
        path = self.root / name
        path.write_text(f"#!{sys.executable}\n" + source)
        path.chmod(0o755)
        return path

    def run_binary(self, source: str, *, timeout: float = 5) -> dict:
        executable = self.executable("binary", source)
        return collector.run_one(
            "test-package",
            "test-target",
            executable,
            str(self.root),
            self.root,
            timeout=timeout,
        )

    def test_success_and_ignored_are_distinct(self) -> None:
        result = self.run_binary(
            "print('test ran ... ok')\nprint('test absent ... ignored, FIXTURE: sample')\n"
        )
        self.assertEqual(result["passed"], ["ran"])
        self.assertEqual(
            result["ignored"], [{"test": "absent", "reason": "FIXTURE: sample"}]
        )
        self.assertFalse(result["crashed"])

    def test_assertion_failure_retains_output(self) -> None:
        result = self.run_binary(
            "print('test bad ... FAILED')\nprint('assertion evidence')\nraise SystemExit(101)\n"
        )
        self.assertEqual(result["failed"], ["bad"])
        self.assertIn("assertion evidence", result["output"])
        self.assertEqual(result["returncode"], 101)

    def test_missing_fixture_does_not_count_as_pass(self) -> None:
        result = self.run_binary(
            "import os\nfrom pathlib import Path\nPath(os.environ['EMU198X_SKIP_LOG']).write_text('needs_rom\\tMissing fixture\\n')\nprint('test needs_rom ... ok')\n"
        )
        self.assertEqual(result["passed"], [])
        self.assertEqual(
            result["skipped"], [{"test": "needs_rom", "reason": "Missing fixture"}]
        )
        self.assertEqual(result["returncode"], 0)

    def test_timeout_discards_partial_pass_credit_and_keeps_logs(self) -> None:
        result = self.run_binary(
            "import time\nprint('test partial ... ok', flush=True)\nprint('before hang', flush=True)\ntime.sleep(20)\n",
            timeout=0.3,
        )
        self.assertEqual(result["passed"], [])
        self.assertTrue(result["crashed"])
        self.assertNotEqual(result["returncode"], 0)
        self.assertIn("before hang", result["output"])
        self.assertIn("Host deadline exceeded", result["output"])

    @unittest.skipIf(
        os.name == "nt", "POSIX process-group assertion; Windows taskkill is separate"
    )
    def test_timeout_kills_descendant_even_after_parent_exits(self) -> None:
        marker = self.root / "child-survived"
        child = (
            "import time; from pathlib import Path; time.sleep(1); Path("
            + repr(str(marker))
            + ").write_text('survived')"
        )
        result = self.run_binary(
            "import subprocess, sys\nsubprocess.Popen([sys.executable, '-c', "
            + repr(child)
            + "])\nprint('test partial ... ok', flush=True)\n",
            timeout=0.4,
        )
        self.assertTrue(result["crashed"])
        self.assertEqual(result["passed"], [])
        time.sleep(1.1)
        self.assertFalse(
            marker.exists(), "descendant survived the process-group deadline"
        )

    def test_output_drain_itself_is_bounded(self) -> None:
        proc = MagicMock()
        proc.returncode = 0
        proc.poll.return_value = 0
        proc.communicate.side_effect = [
            subprocess.TimeoutExpired(["fixture"], 0),
            subprocess.TimeoutExpired(
                ["fixture"], 10, output=b"partial", stderr=b"detail"
            ),
        ]
        with (
            patch.object(collector.subprocess, "Popen", return_value=proc),
            patch.object(collector, "stop_process_tree") as stop,
            self.assertRaises(subprocess.TimeoutExpired) as caught,
        ):
            collector.bounded_run(["fixture"], cwd=self.root, timeout=0)
        self.assertEqual(caught.exception.stdout, "partial")
        self.assertIn("cleanup unconfirmed", caught.exception.stderr)
        self.assertEqual(proc.communicate.call_args.kwargs["timeout"], 10)
        stop.assert_called_once_with(proc)
        proc.stdout.close.assert_called_once()
        proc.stderr.close.assert_called_once()

    def test_failed_cleanup_wait_still_closes_output_pipes(self) -> None:
        proc = MagicMock()
        proc.poll.return_value = None
        proc.communicate.return_value = ("partial", "")
        proc.wait.side_effect = subprocess.TimeoutExpired(["fixture"], 10)
        with (
            patch.object(collector.subprocess, "Popen", return_value=proc),
            patch.object(collector, "stop_process_tree"),
            self.assertRaises(subprocess.TimeoutExpired),
        ):
            collector.bounded_run(["fixture"], cwd=self.root, timeout=1)
        proc.wait.assert_called_once_with(timeout=10)
        proc.stdout.close.assert_called_once()
        proc.stderr.close.assert_called_once()

    def test_phase_progress_and_timeout(self) -> None:
        output = io.StringIO()
        with (
            patch.object(collector, "REPO", self.root),
            patch.object(collector, "HEARTBEAT_SECONDS", 0.03),
            contextlib.redirect_stderr(output),
            self.assertRaisesRegex(SystemExit, "timed out"),
        ):
            collector.run(
                [sys.executable, "-c", "import time; time.sleep(20)"],
                timeout=0.12,
                label="build",
            )
        self.assertIn("start build", output.getvalue())
        self.assertIn("active build", output.getvalue())
        self.assertIn("Host deadline exceeded", output.getvalue())

    def test_failed_metadata_is_not_parsed_as_success(self) -> None:
        with (
            patch.object(
                collector,
                "run",
                return_value=subprocess.CompletedProcess(
                    [], 1, "{}", "metadata broken"
                ),
            ),
            contextlib.redirect_stderr(io.StringIO()),
            self.assertRaisesRegex(SystemExit, "metadata failed"),
        ):
            collector.workspace_metadata()

    def test_failed_build_cannot_produce_partial_evidence(self) -> None:
        output = io.StringIO()
        with (
            patch.object(
                collector,
                "run",
                return_value=subprocess.CompletedProcess(
                    [], 1, "compiler diagnostic", "compile broken"
                ),
            ),
            contextlib.redirect_stderr(output),
            self.assertRaisesRegex(SystemExit, "evidence would be partial"),
        ):
            collector.test_executables({})
        self.assertIn("compiler diagnostic", output.getvalue())
        self.assertIn("compile broken", output.getvalue())

    def test_unparsed_nonzero_test_exit_is_still_failure(self) -> None:
        result = self.run_binary("raise SystemExit(101)\n")
        self.assertTrue(result["crashed"])

    def test_ledger_survives_failure_and_other_binaries_still_run(self) -> None:
        registry = self.root / "systems.toml"
        registry.write_text('[[system]]\nmachine_id="fixture"\ncrate="package"\n')
        good = self.executable("good", "print('test proved ... ok')\n")
        bad = self.executable(
            "bad", "print('test broken ... FAILED')\nraise SystemExit(101)\n"
        )
        hang = self.executable(
            "hang",
            "import time\nprint('test unfinished ... ok', flush=True)\ntime.sleep(20)\n",
        )
        run_one = collector.run_one
        output = self.root / "evidence.json"
        progress = io.StringIO()
        with (
            patch.object(collector, "REGISTRY", registry),
            patch.object(collector, "HEARTBEAT_SECONDS", 0.03),
            patch.object(
                collector,
                "run_one",
                side_effect=lambda *args: run_one(*args, timeout=0.2),
            ),
            patch.object(
                collector,
                "workspace_metadata",
                return_value=(
                    {"package": {"package"}},
                    {},
                    {"package": str(self.root)},
                ),
            ),
            patch.object(
                collector,
                "test_executables",
                return_value=[
                    ("package", "bad", bad),
                    ("package", "hang", hang),
                    ("package", "good", good),
                ],
            ),
            patch.object(
                sys, "argv", ["collector", "--out", str(output), "--jobs", "1"]
            ),
            contextlib.redirect_stderr(progress),
        ):
            self.assertEqual(collector.main(), 1)
        ledger = json.loads(output.read_text())
        self.assertEqual(
            set(ledger),
            {"systems", "packages", "ignored_reasons", "unattributed_packages"},
        )
        self.assertEqual(ledger["packages"]["package"]["passed"], 1)
        self.assertEqual(ledger["packages"]["package"]["failed"], 1)
        self.assertIn("start binary package (bad)", progress.getvalue())
        self.assertIn("end binary package (good): completed", progress.getvalue())
        self.assertIn("progress:", progress.getvalue())
        self.assertIn("1 active [package (hang):", progress.getvalue())
        self.assertIn("1 queued", progress.getvalue())
        self.assertIn("Host deadline exceeded", progress.getvalue())


if __name__ == "__main__":
    unittest.main()
