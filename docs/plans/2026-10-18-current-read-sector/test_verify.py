"""Failure controls for captured ROM evidence inspection."""

import json
import tarfile
import unittest

from verify import HERE, verify


class EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        with tarfile.open(HERE / "evidence/runs.tar.gz") as archive:
            self.files = {
                m.name: archive.extractfile(m).read() for m in archive if m.isfile()
            }

    def change(self, name, mutate) -> None:
        record = json.loads(self.files[name])
        mutate(record)
        self.files[name] = json.dumps(record).encode()

    def test_complete(self) -> None:
        self.assertEqual(verify(self.files)["rom_passes"], 2)

    def test_zero_assertions_rejects(self) -> None:
        self.change(
            "rom-1/regression.json",
            lambda r: r["stages"]["execution"].update(assertions=0),
        )
        with self.assertRaisesRegex(ValueError, "assertions"):
            verify(self.files)

    def test_successful_wrong_result_rejects(self) -> None:
        self.change(
            "rom-wrong/regression.json", lambda r: r.update(regression_passed=True)
        )
        with self.assertRaisesRegex(ValueError, "control"):
            verify(self.files)

    def test_changed_source_rejects(self) -> None:
        self.change(
            "rom-2/regression.json", lambda r: r["tools"]["emu"].update(commit="other")
        )
        with self.assertRaisesRegex(ValueError, "identity"):
            verify(self.files)

    def test_missing_execution_writes_rejects(self) -> None:
        self.change(
            "rom-1/regression.json",
            lambda r: r["stages"]["execution"]["observed"].update(writes=[]),
        )
        with self.assertRaisesRegex(ValueError, "transition"):
            verify(self.files)

    def test_false_qualification_rejects(self) -> None:
        self.change("rom-1/regression.json", lambda r: r.update(qualified=True))
        with self.assertRaisesRegex(ValueError, "scope"):
            verify(self.files)


if __name__ == "__main__":
    unittest.main()
