# SPDX-License-Identifier: GPL-2.0-or-later
"""Reject fabricated successful captured evidence even when rehashed."""

import json
import tarfile
import unittest
from pathlib import Path

from verify import verify


class EvidenceChecks(unittest.TestCase):
    def files(self):
        with tarfile.open(Path(__file__).parent / "evidence/runs.tar.gz") as bundle:
            return {member.name: bundle.extractfile(member).read() for member in bundle}

    def test_complete(self):
        self.assertEqual(verify(self.files())["fresh_rom_passes"], 2)

    def test_semantic_changes_are_rejected(self):
        changes = [
            ("reference/result.json", lambda r: r.update(cases=[])),
            ("reference/result.json", lambda r: r.update(qualified=True)),
            (
                "reference/result.json",
                lambda r: r["cases"][0]["observed"].update(data=[]),
            ),
            ("rom-wrong/regression.json", lambda r: r.update(regression_passed=True)),
            (
                "rom-1/regression.json",
                lambda r: r["stages"]["execution"]["observed"].update(writes=[]),
            ),
            (
                "rom-1/regression.json",
                lambda r: r["stages"]["loading"].update(assertions=0),
            ),
            (
                "rom-2/regression.json",
                lambda r: r["tools"]["emu"].update(commit="unknown"),
            ),
        ]
        for name, mutate in changes:
            with self.subTest(name=name):
                files = self.files()
                record = json.loads(files[name])
                mutate(record)
                files[name] = json.dumps(record).encode()
                with self.assertRaises(ValueError):
                    verify(files)
