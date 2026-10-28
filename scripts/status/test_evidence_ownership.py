"""Board evidence must not become evidence for its sibling by sharing an app."""

import unittest

from evidence_ownership import partitions
from render_status import render


class BoardEvidenceTests(unittest.TestCase):
    def test_sibling_board_tests_are_neither_owned_nor_shared_evidence(self) -> None:
        systems = [
            {"machine_id": "ntsc", "machine_crate": "machine-ntsc", "crate": "app"},
            {"machine_id": "pal", "machine_crate": "machine-pal", "crate": "app"},
            {"machine_id": "other", "crate": "other-app"},
        ]
        closures = {
            "app": {"app", "runtime", "common", "machine-ntsc", "machine-pal", "chip"},
            "machine-ntsc": {"machine-ntsc", "common", "chip"},
            "machine-pal": {"machine-pal", "common", "chip"},
            "other-app": {"other-app", "chip"},
        }
        result = partitions(systems, closures)
        self.assertEqual(
            result["ntsc"],
            (["machine-ntsc"], ["app", "chip", "common", "runtime"], ["pal"]),
        )
        self.assertEqual(
            result["pal"],
            (["machine-pal"], ["app", "chip", "common", "runtime"], ["ntsc"]),
        )
        self.assertEqual(result["other"], (["other-app"], ["chip"], []))

    def test_legacy_shared_binary_stays_explicitly_ambiguous(self) -> None:
        systems = [{"machine_id": name, "crate": "app"} for name in ("one", "two")]
        result = partitions(systems, {"app": {"app", "unsplit-machine"}})
        self.assertEqual(result["one"], (["app", "unsplit-machine"], [], ["two"]))

    def test_invalid_or_unreachable_owner_cannot_create_zero_test_success(self) -> None:
        for root in ("machine-absent", "common", "machine-orphan"):
            with self.subTest(root=root), self.assertRaises(ValueError):
                partitions(
                    [{"machine_id": "one", "crate": "app", "machine_crate": root}],
                    {
                        "app": {"app"},
                        "machine-orphan": {"machine-orphan"},
                        "common": {"common"},
                    },
                )

    def test_two_machine_ids_cannot_claim_the_same_concrete_crate(self) -> None:
        with self.assertRaises(ValueError):
            partitions(
                [
                    {"machine_id": name, "crate": "app", "machine_crate": "machine-one"}
                    for name in ("one", "two")
                ],
                {"app": {"app", "machine-one"}, "machine-one": {"machine-one"}},
            )


class BoardStatusTests(unittest.TestCase):
    def test_registered_boards_are_distinct_even_with_one_family_binary(self) -> None:
        systems = [
            {
                "machine_id": name,
                "machine_crate": f"machine-{name}",
                "crate": "app",
                "label": "system:sord-m5",
                "milestone": "Sord M5 100%",
            }
            for name in ("ntsc", "pal")
        ]
        closures = {
            "app": {"app", "machine-ntsc", "machine-pal", "common"},
            "machine-ntsc": {"machine-ntsc", "common"},
            "machine-pal": {"machine-pal", "common"},
        }
        text, _ = render(systems, closures)
        self.assertIn("| `ntsc` |", text)
        self.assertIn("| `pal` |", text)
        self.assertNotIn("| `ntsc` ⚠️", text)
        self.assertNotIn("| `pal` ⚠️", text)


if __name__ == "__main__":
    unittest.main()
