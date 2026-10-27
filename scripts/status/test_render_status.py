#!/usr/bin/env python3
"""Machine identifiers remain reusable independently of display warnings."""
import importlib.util
from pathlib import Path
import re
import unittest

spec = importlib.util.spec_from_file_location("render_status", Path(__file__).with_name("render_status.py"))
renderer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(renderer)


class MachineIdentityTests(unittest.TestCase):
    def test_code_span_ids_match_registry_even_when_machines_share_a_crate(self):
        systems = [
            {"machine_id": "commodore-c64", "crate": "emu198x-c64", "label": "system:c64", "milestone": "C64 100%"},
            {"machine_id": "commodore-c128", "crate": "emu198x-c64", "label": "system:c128", "milestone": "C128 100%"},
            {"machine_id": "sinclair-zx-spectrum", "crate": "emu198x-spectrum", "label": "system:spectrum", "milestone": "Spectrum 100%"},
        ]
        text, _ = renderer.render(systems, {"emu198x-c64": {"emu198x-c64"}, "emu198x-spectrum": {"emu198x-spectrum"}})
        identifiers = re.findall(r"^\| `([^`]+)`", text, re.MULTILINE)
        self.assertEqual(set(identifiers), {s["machine_id"] for s in systems})
        for machine in ["commodore-c64", "commodore-c128"]:
            self.assertIn(f"| `{machine}` ⚠️ |", text)
        self.assertIn("| `sinclair-zx-spectrum` |", text)


if __name__ == "__main__":
    unittest.main()
