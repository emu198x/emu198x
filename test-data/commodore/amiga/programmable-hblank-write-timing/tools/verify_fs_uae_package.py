#!/usr/bin/env python3
"""Independently remeasure the retained FS-UAE write-timing APNG package."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

from PIL import Image


SCHEMA_VERSION = "1.0.0"
WIDTH = 756
HEIGHT = 576
FRAME_COUNT = 3
STORAGE_EXCLUSION = (0, 2)
PROFILES = ("ecs", "aga")
CASES = (
    "midline-hbstrt-past",
    "midline-hbstop-future",
    "midline-ecsena-enable",
    "midline-extblken-enable",
    "midline-blanken-enable",
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


class VerificationError(ValueError):
    """One retained package invariant failed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path, context: str) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
    except OSError as error:
        raise VerificationError(f"{context}: file is missing or unreadable") from error
    return digest.hexdigest()


def read_bytes(path: Path, context: str) -> bytes:
    try:
        return path.read_bytes()
    except OSError as error:
        raise VerificationError(f"{context}: file is missing or unreadable") from error


def read_json(path: Path, context: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError(f"{context}: unreadable JSON") from error
    require(isinstance(value, dict), f"{context}: expected a JSON object")
    return value


def checked_package_path(root: Path, value: Any, context: str) -> Path:
    require(isinstance(value, str) and value, f"{context}: path is missing")
    relative = PurePosixPath(value)
    require(not relative.is_absolute(), f"{context}: path must be relative")
    require(".." not in relative.parts, f"{context}: path escapes package")
    return root.joinpath(*relative.parts)


def require_sha(value: Any, context: str) -> str:
    require(
        isinstance(value, str) and SHA256_RE.fullmatch(value) is not None,
        f"{context}: invalid SHA-256",
    )
    return value


def rgb4(word: str) -> tuple[int, int, int]:
    value = int(word, 16)
    return (
        ((value >> 8) & 0xF) * 17,
        ((value >> 4) & 0xF) * 17,
        (value & 0xF) * 17,
    )


def pixel_rgb(raw: bytes, x: int, y: int) -> tuple[int, int, int]:
    offset = (y * WIDTH + x) * 4
    return raw[offset], raw[offset + 1], raw[offset + 2]


def labelled_runs(
    raw: bytes,
    y: int,
    guard: tuple[int, int, int],
    marker: tuple[int, int, int],
    context: str,
) -> list[tuple[str, int, int]]:
    labels: list[str] = []
    for x in range(STORAGE_EXCLUSION[1], WIDTH):
        rgb = pixel_rgb(raw, x, y)
        if rgb == (0, 0, 0):
            label = "blank"
        elif rgb == guard:
            label = "guard"
        elif rgb == marker:
            label = "marker"
        else:
            raise VerificationError(
                f"{context}: unexpected RGB at semantic sample {x},{y}"
            )
        labels.append(label)

    runs: list[tuple[str, int, int]] = []
    start = STORAGE_EXCLUSION[1]
    current = labels[0]
    for x, label in enumerate(labels[1:], start=STORAGE_EXCLUSION[1] + 1):
        if label != current:
            runs.append((current, start, x))
            current = label
            start = x
    runs.append((current, start, WIDTH))
    return runs


def derive_semantics(
    raw: bytes,
    guard_word: str,
    marker_word: str,
    context: str,
) -> dict[str, Any]:
    require(
        len(raw) == WIDTH * HEIGHT * 4,
        f"{context}: decoded frame has the wrong byte count",
    )
    guard = rgb4(guard_word)
    marker = rgb4(marker_word)
    candidates: list[int] = []
    for y in range(2, HEIGHT - 2):
        has_guard = False
        has_marker = False
        for x in range(STORAGE_EXCLUSION[1], WIDTH):
            rgb = pixel_rgb(raw, x, y)
            has_guard = has_guard or rgb == guard
            has_marker = has_marker or rgb == marker
            if has_guard and has_marker:
                candidates.append(y)
                break
    require(
        len(candidates) == 2 and candidates[1] == candidates[0] + 1,
        f"{context}: expected one doubled mutation-marker row",
    )
    mutation_row = candidates[0]

    lines: list[dict[str, Any]] = []
    for role, y in (
        ("pre-mutation baseline", mutation_row - 2),
        ("mutation output", mutation_row),
        ("post-mutation control", mutation_row + 2),
    ):
        row_bytes = WIDTH * 4
        first = raw[y * row_bytes : (y + 1) * row_bytes]
        second = raw[(y + 1) * row_bytes : (y + 2) * row_bytes]
        require(first == second, f"{context}: semantic row is not doubled")
        for x in range(*STORAGE_EXCLUSION):
            require(
                pixel_rgb(raw, x, y) == (0, 0, 0),
                f"{context}: storage exclusion is not black",
            )
        runs = labelled_runs(raw, y, guard, marker, context)
        lines.append(
            {
                "role": role,
                "raw_rows": [y, y + 1],
                "black_runs": [
                    [start, stop]
                    for label, start, stop in runs
                    if label == "blank"
                ],
                "guard_runs": [
                    [start, stop]
                    for label, start, stop in runs
                    if label == "guard"
                ],
                "marker_runs": [
                    [start, stop]
                    for label, start, stop in runs
                    if label == "marker"
                ],
            }
        )

    mutation = lines[1]
    require(
        bool(mutation["marker_runs"]),
        f"{context}: mutation output has no marker run",
    )
    return {
        "mutation_output_rows": [mutation_row, mutation_row + 1],
        "marker_start_sample": mutation["marker_runs"][0][0],
        "lines": lines,
    }


def decode_apng(path: Path, context: str) -> list[bytes]:
    try:
        with Image.open(path) as image:
            require(
                getattr(image, "n_frames", 1) == FRAME_COUNT,
                f"{context}: APNG must contain three frames",
            )
            frames: list[bytes] = []
            for index in range(FRAME_COUNT):
                image.seek(index)
                require(
                    image.size == (WIDTH, HEIGHT),
                    f"{context}: APNG dimensions changed",
                )
                frames.append(image.convert("RGBA").tobytes())
            return frames
    except OSError as error:
        raise VerificationError(f"{context}: APNG cannot be decoded") from error


def verify_run(
    package_root: Path,
    run: dict[str, Any],
    case: dict[str, Any],
) -> dict[str, Any]:
    profile = run.get("profile")
    case_id = run.get("case_id")
    require(profile in PROFILES, "run has an unknown profile")
    require(case_id in CASES, f"{profile}: run has an unknown case")
    context = f"{profile}/{case_id}"

    capture_path = checked_package_path(
        package_root, run.get("capture_file"), f"{context}: capture"
    )
    record_path = checked_package_path(
        package_root, run.get("record_file"), f"{context}: record"
    )
    expected_capture_sha = require_sha(
        run.get("capture_sha256"), f"{context}: capture"
    )
    expected_record_sha = require_sha(
        run.get("record_sha256"), f"{context}: record"
    )
    require(
        sha256_file(capture_path, f"{context}: capture") == expected_capture_sha,
        f"{context}: capture SHA-256 mismatch",
    )
    require(
        sha256_file(record_path, f"{context}: record") == expected_record_sha,
        f"{context}: record SHA-256 mismatch",
    )

    record = read_json(record_path, f"{context}: record")
    require(record.get("schema_version") == SCHEMA_VERSION, f"{context}: schema")
    require(record.get("case_id") == case_id, f"{context}: record case")
    require(
        str(record.get("machine", {}).get("chipset", "")).lower() == profile,
        f"{context}: record profile",
    )
    source_capture = record.get("source_capture")
    observations = record.get("observations")
    require(isinstance(source_capture, dict), f"{context}: source_capture")
    require(isinstance(observations, dict), f"{context}: observations")
    visual = case.get("identity", {}).get("visual", {})
    require(
        observations.get("guard_color_word") == visual.get("color00"),
        f"{context}: guard colour disagrees with case source",
    )
    require(
        observations.get("marker_color_word") == visual.get("marker_color00"),
        f"{context}: marker colour disagrees with case source",
    )
    require(source_capture.get("width") == WIDTH, f"{context}: width")
    require(source_capture.get("height") == HEIGHT, f"{context}: height")
    require(source_capture.get("stride_bytes") == WIDTH * 4, f"{context}: stride")
    require(
        source_capture.get("file_name") == f"../{run['capture_file']}",
        f"{context}: record capture name",
    )
    require(
        source_capture.get("file_sha256") == expected_capture_sha,
        f"{context}: record capture SHA-256",
    )

    frames = decode_apng(capture_path, context)
    require(len(set(frames)) == 1, f"{context}: adjacent APNG frames differ")
    decoded_sha = sha256_bytes(b"".join(frames))
    require(
        decoded_sha == require_sha(run.get("decoded_pixel_sha256"), context),
        f"{context}: decoded-pixel SHA-256 mismatch",
    )
    require(
        decoded_sha == source_capture.get("decoded_pixel_sha256"),
        f"{context}: record decoded-pixel SHA-256 mismatch",
    )

    derived = derive_semantics(
        frames[0],
        visual.get("color00"),
        visual.get("marker_color00"),
        context,
    )
    require(
        derived["mutation_output_rows"] == run.get("mutation_output_rows"),
        f"{context}: mutation rows disagree with package",
    )
    require(
        derived["lines"] == observations.get("lines"),
        f"{context}: pixel-derived semantic lines disagree with record",
    )
    write_evidence = (
        record.get("stimulus", {}).get("write_position_evidence", {})
    )
    timed_write = case.get("timed_write", {})
    register = str(timed_write.get("register", "")).lower()
    expected_stimulus = {
        "reset_beam_line": timed_write.get("reset_beam_line"),
        "reset_wait_hpos_cck": timed_write.get("reset_hpos_cck"),
        "mutation_beam_line": timed_write.get("beam_line"),
        "mutation_wait_hpos_cck": timed_write.get("wait_hpos_cck"),
        "tested_register": timed_write.get("register"),
        "baseline_word": case.get("registers", {}).get(register, {}).get("word"),
        "mutation_word": timed_write.get("word"),
    }
    stimulus = record.get("stimulus", {})
    require(
        {key: stimulus.get(key) for key in expected_stimulus} == expected_stimulus,
        f"{context}: stimulus disagrees with case source",
    )
    require(
        derived["marker_start_sample"]
        == write_evidence.get("marker_start_sample"),
        f"{context}: marker start disagrees with record",
    )
    require(
        record.get("execution", {}).get("adjacent_field_stability") == "confirmed",
        f"{context}: record does not claim confirmed field stability",
    )
    require(
        record.get("normalization", {}).get("alignment_search") is False,
        f"{context}: alignment search must remain disabled",
    )
    require(
        observations.get("uncertainty_samples") == 0,
        f"{context}: sample uncertainty must remain zero",
    )

    semantic_bytes = json.dumps(
        derived, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return {
        "profile": profile,
        "case_id": case_id,
        "capture_sha256": expected_capture_sha,
        "decoded_pixel_sha256": decoded_sha,
        "semantic_sha256": sha256_bytes(semantic_bytes),
        "frame_count": len(frames),
        "adjacent_frame_stability": "byte-identical",
        "mutation_output_rows": derived["mutation_output_rows"],
    }


def verify_package(package_root: Path) -> dict[str, Any]:
    manifest_path = package_root / "package-v1.json"
    package_bytes = read_bytes(manifest_path, "package manifest")
    package = read_json(manifest_path, "package manifest")
    require(package.get("schema_version") == SCHEMA_VERSION, "package schema")
    matrix = package.get("matrix")
    runs = package.get("runs")
    require(isinstance(matrix, dict), "package matrix is missing")
    require(isinstance(runs, list), "package runs are missing")
    require(matrix.get("profiles") == list(PROFILES), "package profiles changed")
    require(matrix.get("cases") == list(CASES), "package cases changed")
    require(matrix.get("run_count") == len(PROFILES) * len(CASES), "run count")
    require(matrix.get("raw_width") == WIDTH, "package width changed")
    require(matrix.get("raw_height") == HEIGHT, "package height changed")
    require(matrix.get("packaged_pixel_format") == "RGBA8888", "pixel format")

    case_source_path = package_root.parent.parent / "cases/cases.json"
    case_source_bytes = read_bytes(case_source_path, "case source")
    case_source = read_json(case_source_path, "case source")
    source_suite = case_source.get("suite", {})
    require(
        source_suite.get("id") == package.get("suite", {}).get("id"),
        "case-source suite ID differs from package",
    )
    require(
        source_suite.get("version") == package.get("suite", {}).get("version"),
        "case-source suite version differs from package",
    )
    source_cases = case_source.get("cases")
    require(isinstance(source_cases, list), "case source has no cases")
    cases_by_id = {case.get("id"): case for case in source_cases}
    require(set(cases_by_id) == set(CASES), "case-source matrix changed")

    expected = {(profile, case_id) for profile in PROFILES for case_id in CASES}
    actual = {(run.get("profile"), run.get("case_id")) for run in runs}
    require(actual == expected and len(runs) == len(expected), "run matrix is incomplete")
    verified = [
        verify_run(package_root, run, cases_by_id[run["case_id"]]) for run in runs
    ]

    referenced_captures = {run["capture_file"] for run in runs}
    retained_captures = {
        path.relative_to(package_root).as_posix()
        for path in (package_root / "captures").glob("*.apng")
    }
    require(
        retained_captures == referenced_captures,
        "retained APNG set differs from package manifest",
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "pass",
        "package_manifest_sha256": sha256_bytes(package_bytes),
        "case_source_sha256": sha256_bytes(case_source_bytes),
        "run_count": len(verified),
        "frame_count": len(verified) * FRAME_COUNT,
        "all_adjacent_frames_byte_identical": True,
        "runs": verified,
    }


def parse_args() -> argparse.Namespace:
    default_package = Path(__file__).resolve().parent.parent / "references"
    default_package /= "fs-uae-5.0.7-f362278c"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package_root", type=Path, nargs="?", default=default_package)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = verify_package(args.package_root)
    except (OSError, TypeError, VerificationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
