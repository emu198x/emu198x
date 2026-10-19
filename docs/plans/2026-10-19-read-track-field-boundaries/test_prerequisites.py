# SPDX-License-Identifier: GPL-2.0-or-later
"""Exercise the real runner preflight with a separate, empty firmware home."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


class PrerequisiteControl(unittest.TestCase):
    def test_empty_firmware_home_is_not_a_pass(self) -> None:
        spec = importlib.util.spec_from_file_location("fields_runner", HERE / "run.py")
        if spec is None or spec.loader is None:
            raise ValueError("runner source missing")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory(
            prefix="missing-control-", dir=HERE
        ) as temporary:
            root = Path(temporary)
            (root / "plan.md").write_bytes((HERE / "plan.md").read_bytes())
            runner.HERE = root
            with patch.object(Path, "home", return_value=root / "empty-home"):
                runner.main()
            record = json.loads((root / "runs/campaign.json").read_text())
            self.assertEqual(record["status"], "missing_prerequisite")
            self.assertEqual(len(record["missing"]), 4)
            self.assertEqual(record["commands"], [])
            self.assertEqual(record["observations"], [])
            self.assertFalse(record["qualified"])


if __name__ == "__main__":
    unittest.main()
