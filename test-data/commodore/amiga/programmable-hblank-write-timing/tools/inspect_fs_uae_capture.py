#!/usr/bin/env python3
"""Summarise the marked sample line in one FS-UAE raw capture set."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

WIDTH = 756
HEIGHT = 576
BYTES_PER_PIXEL = 4
LEFT_STORAGE_PADDING = 2


def rgb12(word: str) -> tuple[int, int, int]:
    value = int(word, 16)
    red = ((value >> 8) & 0xF) * 17
    green = ((value >> 4) & 0xF) * 17
    blue = (value & 0xF) * 17
    return red, green, blue


def row_signature(
    raw: bytes,
    y: int,
    guard: tuple[int, int, int],
    marker: tuple[int, int, int],
) -> tuple[tuple[str, int, int], ...]:
    labels: list[str] = []
    offset = y * WIDTH * BYTES_PER_PIXEL
    for x in range(LEFT_STORAGE_PADDING, WIDTH):
        blue, green, red, _alpha = raw[
            offset + x * BYTES_PER_PIXEL : offset + (x + 1) * BYTES_PER_PIXEL
        ]
        rgb = red, green, blue
        if rgb == (0, 0, 0):
            label = "blank"
        elif rgb == guard:
            label = "guard"
        elif rgb == marker:
            label = "marker"
        else:
            label = f"other-{red:02x}{green:02x}{blue:02x}"
        labels.append(label)

    runs: list[tuple[str, int, int]] = []
    start = LEFT_STORAGE_PADDING
    current = labels[0]
    for x, label in enumerate(labels[1:], start=LEFT_STORAGE_PADDING + 1):
        if label != current:
            runs.append((current, start, x))
            current = label
            start = x
    runs.append((current, start, WIDTH))
    return tuple(runs)


def inspect_run(run_dir: Path, suite: dict[str, object]) -> None:
    case_id = run_dir.name
    cases = suite["cases"]
    assert isinstance(cases, list)
    case = next(record for record in cases if record["id"] == case_id)
    visual = case["identity"]["visual"]
    guard = rgb12(visual["color00"])
    marker = rgb12(visual["marker_color00"])

    raw_paths = sorted((run_dir / "capture").glob("field-*.bgra"))
    if len(raw_paths) != 3:
        raise ValueError(f"{run_dir}: expected three raw fields")
    raw_fields = [path.read_bytes() for path in raw_paths]
    if any(len(raw) != WIDTH * HEIGHT * BYTES_PER_PIXEL for raw in raw_fields):
        raise ValueError(f"{run_dir}: raw field has the wrong size")
    if len(set(raw_fields)) != 1:
        raise ValueError(f"{run_dir}: adjacent raw fields differ")

    raw = raw_fields[0]
    signatures = [
        row_signature(raw, y, guard, marker)
        for y in range(194, 213)
    ]
    groups: list[tuple[int, int, tuple[tuple[str, int, int], ...]]] = []
    start = 194
    current = signatures[0]
    for y, signature in zip(range(195, 213), signatures[1:], strict=True):
        if signature != current:
            groups.append((start, y, current))
            start = y
            current = signature
    groups.append((start, 213, current))

    print(f"{run_dir.parent.name}/{case_id}")
    for start, stop, signature in groups:
        rendered = ", ".join(
            f"{label}[{left},{right})"
            for label, left, right in signature
        )
        print(f"  rows [{start},{stop}): {rendered}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("capture_root", type=Path)
    parser.add_argument("suite_manifest", type=Path)
    args = parser.parse_args()

    suite = json.loads(args.suite_manifest.read_text(encoding="utf-8"))
    for profile in ("ecs", "aga"):
        profile_dir = args.capture_root / profile
        for run_dir in sorted(path for path in profile_dir.iterdir() if path.is_dir()):
            inspect_run(run_dir, suite)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
