"""The counter-origin gate must reject missing or inconsistent observations."""

import unittest

from compare_phase import traced_origin


def trace(padding: int = 4) -> str:
    return "\n".join(
        f"ECS_ORIGIN guest={field} v={line} counter=100 "
        f"x={400 - 368 + padding} shift={padding} lol=0 ecs=1"
        for field in (9, 10, 11)
        for line in range(44, 244)
    )


class OriginTests(unittest.TestCase):
    def test_uses_counter_and_storage_coordinates(self) -> None:
        # Different traced padding must produce a different mapping, without
        # looking at pixels or assuming that every reference has this offset.
        self.assertEqual(traced_origin(trace(0), 368), (368, 0))
        self.assertEqual(traced_origin(trace(4), 368), (364, 4))

    def test_missing_or_duplicate_rows_fail(self) -> None:
        full = trace()
        for candidate in (
            "",
            "\n".join(full.splitlines()[1:]),
            full + "\n" + full.splitlines()[0],
        ):
            with self.assertRaises(ValueError):
                traced_origin(candidate, 368)

    def test_inconsistent_coordinates_fail(self) -> None:
        for candidate in (
            trace().replace("x=36", "x=40", 1),
            trace().replace("lol=0", "lol=1", 1),
            trace().replace("x=36 shift=4", "x=32 shift=0", 1),
        ):
            with self.assertRaises(ValueError):
                traced_origin(candidate, 368)


if __name__ == "__main__":
    unittest.main()
