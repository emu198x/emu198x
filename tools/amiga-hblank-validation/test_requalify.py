"""Positive and tampered-input checks for retained reference admission."""

from __future__ import annotations

import shutil
import tempfile
import unittest
from pathlib import Path

from requalify import PRODUCERS, requalify

ROOT = (
    Path(__file__).resolve().parents[2]
    / "test-data/commodore/amiga/programmable-hblank/references"
)


class ReferenceAdmissionTests(unittest.TestCase):
    def test_complete_registered_matrix_has_both_agreements_and_disagreements(
        self,
    ) -> None:
        report = requalify(ROOT)
        self.assertEqual(report["compared_reference_frames"], 84)
        self.assertEqual(len(report["runs"]), 14)
        self.assertEqual(sum(row["comparator_consensus"] for row in report["runs"]), 9)

    def test_changed_producer_pixel_artifact_cannot_be_admitted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for producer in PRODUCERS:
                shutil.copytree(ROOT / producer, root / producer)
            image = root / PRODUCERS[0] / "captures/ecs--programmed-central.apng"
            data = bytearray(image.read_bytes())
            data[-1] ^= 1
            image.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "changed capture"):
                requalify(root)


if __name__ == "__main__":
    unittest.main()
