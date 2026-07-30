#!/usr/bin/env python3
"""Verify and package the FS-UAE programmable-HBLANK write-timing matrix."""

from __future__ import annotations

import argparse
import binascii
import hashlib
import json
import re
import shutil
import stat
import struct
import sys
import zlib
from pathlib import Path
from typing import Any

import PIL
from PIL import Image, ImageSequence

SUITE_ID = "org.198x.amiga.programmable-hblank-write-timing"
SUITE_VERSION = "1.0.0"
SUITE_SOURCE_REVISION = "source-v1"
SUITE_SHA256 = "fc4c4568b3ae5a7d06e07eddb18f9048937717291d022f02cfc32e68d5f6faf3"

PRODUCER = {
    "product": "FS-UAE",
    "version": "5.0.7",
    "revision": "f362278ccd4c60991caac3b4d240d4a3f751bea2",
    "source_url": "https://github.com/FrodeSolheim/fs-uae",
    "uae_base_version": "WinUAE 6.0.1",
    "binary_sha256": (
        "81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b"
    ),
    "capture_patch_sha256": (
        "73e423453152097723b22e4ba0db7cb626b4756e5697d49154bfd98055ddd0ed"
    ),
}
PRODUCER_BUILD_SHA256 = (
    "f3c9a7bc52a91eda942d9befd97861d5f46603d6becb1a833a53c6943d7f4ac0"
)
CAPTURE_TOOLS = {
    "capture.sh": "397c5454b34d6344ce2cd64c5addb94dc6d25f32e9f116cc12e27a7f9c3ff9d3",
    "capture_manifest.py": (
        "61d52dc05e197b3bab95d4e5ce0ccba70841020a943bd83355950187859cc37f"
    ),
    "config.uae.in": (
        "49de327d26d7f2b632bc7a62c987dd625c18acee1cdd00af5558626e3d071678"
    ),
}

WIDTH = 756
HEIGHT = 576
PACKED_STRIDE = WIDTH * 4
PRODUCER_STRIDE = 8192
FRAME_COUNT = 3
GUEST_FIELDS = [9, 10, 11]
SETTLE_FIELDS = 8
STORAGE_EXCLUSION = (0, 2)

FRAMEBUFFER = {
    "stage": (
        "completed UAE chipset video_memory before FS-UAE compatibility crop "
        "and frontend processing"
    ),
    "width": WIDTH,
    "height": HEIGHT,
    "stride_bytes": PRODUCER_STRIDE,
    "depth_bits": 32,
    "pixel_format": "BGRA8888",
    "packed_output_stride_bytes": PACKED_STRIDE,
    "complete": True,
}
FRONTEND_VIEW = {
    "width": 752,
    "height": 572,
    "partial": 572,
    "limits": {"x": 48, "y": 22, "width": 692, "height": 540},
}

CASE_IDS = (
    "midline-hbstrt-past",
    "midline-hbstop-future",
    "midline-ecsena-enable",
    "midline-extblken-enable",
    "midline-blanken-enable",
)
PROFILES = {
    "ecs": {
        "model": "A500 Plus",
        "cpu": "Motorola 68000",
        "agnus": "8375 ECS Agnus",
        "denise_or_lisa": "8373 ECS Denise",
        "chipset": "ECS",
        "ram_bytes": 1_048_576,
        "firmware_revision": "Kickstart 2.04 revision 37.175",
        "firmware_sha256": (
            "d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79"
        ),
        "config": {
            "chipset": "ecs",
            "chipset_compatible": "A500+",
            "chipmem_size": "2",
            "cpu_type": "68000",
            "cpu_model": "68000",
        },
    },
    "aga": {
        "model": "A1200",
        "cpu": "Motorola 68EC020",
        "agnus": "AGA Alice",
        "denise_or_lisa": "AGA Lisa",
        "chipset": "AGA",
        "ram_bytes": 2_097_152,
        "firmware_revision": "Kickstart 3.1 revision 40.068",
        "firmware_sha256": (
            "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"
        ),
        "config": {
            "chipset": "aga",
            "chipset_compatible": "A1200",
            "chipmem_size": "4",
            "cpu_type": "68ec020",
            "cpu_model": "68020",
        },
    },
}

TIMESTAMP_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\+00:00$"
)
SHA_LINE_RE = re.compile(r"^([0-9a-f]{64})  (.+)$")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_equal(actual: Any, expected: Any, context: str) -> None:
    if actual != expected:
        raise ValueError(f"{context}: got {actual!r}, expected {expected!r}")


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def parse_sha256_manifest(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        match = SHA_LINE_RE.fullmatch(line)
        if match is None:
            raise ValueError(f"{path}:{line_number}: malformed SHA-256 line")
        digest, recorded_path = match.groups()
        if recorded_path in entries:
            raise ValueError(f"{path}: duplicate path {recorded_path}")
        entries[recorded_path] = digest
    if not entries:
        raise ValueError(f"{path}: empty SHA-256 manifest")
    return entries


def manifest_by_basename(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for recorded_path, digest in parse_sha256_manifest(path).items():
        name = Path(recorded_path).name
        if name in result:
            raise ValueError(f"{path}: duplicate basename {name}")
        result[name] = digest
    return result


def validate_suite(suite_dir: Path) -> tuple[dict[str, Any], str]:
    suite_path = suite_dir / "suite-v1.json"
    suite_sha256 = sha256_file(suite_path)
    require_equal(suite_sha256, SUITE_SHA256, f"{suite_path}: SHA-256")
    suite = read_json(suite_path)
    require_equal(suite.get("schema_version"), "1.0.0", f"{suite_path}: schema")
    require_equal(
        suite.get("suite"),
        {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": SUITE_SOURCE_REVISION,
            "license": "CC0-1.0",
        },
        f"{suite_path}: suite identity",
    )

    cases = suite.get("cases")
    artifacts = suite.get("artifacts")
    if not isinstance(cases, list) or not isinstance(artifacts, list):
        raise ValueError(f"{suite_path}: cases and artifacts must be arrays")
    require_equal(
        tuple(case["id"] for case in cases),
        CASE_IDS,
        f"{suite_path}: cases",
    )
    require_equal(
        tuple(artifact["case_id"] for artifact in artifacts),
        CASE_IDS,
        f"{suite_path}: artifacts",
    )
    require_equal(
        [case["numeric_id"] for case in cases],
        list(range(1, len(CASE_IDS) + 1)),
        f"{suite_path}: numeric IDs",
    )

    for case, artifact in zip(cases, artifacts, strict=True):
        timed = case["timed_write"]
        require_equal(timed["reset_beam_line"], 127, f"{case['id']}: reset line")
        require_equal(timed["beam_line"], 128, f"{case['id']}: mutation line")
        require_equal(
            case["identity"]["visual"]["method"],
            "scheduled COLOR00 marker",
            f"{case['id']}: marker method",
        )
        for file_key, bytes_key, hash_key in (
            ("adf_file", "adf_bytes", "adf"),
            ("payload_file", "payload_bytes", "payload"),
        ):
            artifact_path = suite_dir / artifact[file_key]
            require_equal(
                artifact_path.stat().st_size,
                artifact[bytes_key],
                f"{artifact_path}: byte count",
            )
            require_equal(
                sha256_file(artifact_path),
                artifact["sha256"][hash_key],
                f"{artifact_path}: SHA-256",
            )
        require_equal(artifact["adf_bytes"], 901_120, f"{case['id']}: ADF size")
    return suite, suite_sha256


def validate_topology(capture_root: Path) -> None:
    actual_profiles = {
        path.name for path in capture_root.iterdir() if not path.name.startswith(".")
    }
    require_equal(actual_profiles, set(PROFILES), f"{capture_root}: profiles")
    for profile in PROFILES:
        profile_dir = capture_root / profile
        actual_cases = {
            path.name for path in profile_dir.iterdir() if not path.name.startswith(".")
        }
        require_equal(actual_cases, set(CASE_IDS), f"{profile_dir}: cases")
        for case_id in CASE_IDS:
            if not (profile_dir / case_id).is_dir():
                raise ValueError(f"{profile_dir / case_id}: expected a directory")


def parse_config(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line_number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise ValueError(f"{path}:{line_number}: malformed configuration line")
        key, value = line.split("=", 1)
        if key in values:
            raise ValueError(f"{path}:{line_number}: duplicate key {key}")
        values[key] = value
    return values


def validate_config(
    path: Path,
    profile: str,
    case_id: str,
    manifest: dict[str, Any],
) -> None:
    values = parse_config(path)
    expected = {
        "config_description": f"198x HBLANK write timing {profile} {case_id}",
        "config_version": "6.0.1",
        "floppy_write_protect": "true",
        "floppy0wp": "true",
        "nr_floppies": "1",
        "sound_output": "none",
        "synchronize_clock": "false",
        "ntsc": "false",
        "cpu_compatible": "true",
        "cpu_cycle_exact": "true",
        "cpu_memory_cycle_exact": "true",
        "blitter_cycle_exact": "true",
        "cycle_exact": "true",
        "gfx_resolution": "hires",
        "gfx_linemode": "double2",
        "gfx_overscanmode": "overscan",
        "gfx_filter": "null",
        "gfx_filter_bilinear": "false",
    }
    expected.update(PROFILES[profile]["config"])
    for key, value in expected.items():
        require_equal(values.get(key), value, f"{path}: {key}")
    require_equal(
        Path(values["kickstart_rom_file"]).resolve(),
        Path(manifest["inputs"]["firmware"]["file"]).resolve(),
        f"{path}: Kickstart path",
    )
    require_equal(
        Path(values["floppy0"]).resolve(),
        Path(manifest["inputs"]["adf"]["file"]).resolve(),
        f"{path}: floppy path",
    )


def expected_ready(case: dict[str, Any]) -> dict[str, Any]:
    registers = case["registers"]
    return {
        "magic": "HBLK",
        "case_number": case["numeric_id"],
        "schema_version": 1,
        "identity": case["identity"]["serial"]["value"][:32],
        "bplcon0": f"0x{int(registers['bplcon0']['word'], 16):04x}",
        "bplcon3": f"0x{int(registers['bplcon3']['word'], 16):04x}",
        "beamcon0": f"0x{int(registers['beamcon0']['word'], 16):04x}",
        "hbstrt": f"0x{int(registers['hbstrt']['word'], 16):04x}",
        "hbstop": f"0x{int(registers['hbstop']['word'], 16):04x}",
        "color00": f"0x{int(case['line_geometry']['guard_color_word'], 16):04x}",
    }


def rgb4(word: str) -> tuple[int, int, int]:
    value = int(word, 16)
    return (
        ((value >> 8) & 0xF) * 17,
        ((value >> 4) & 0xF) * 17,
        (value & 0xF) * 17,
    )


def pixel_rgb(raw: bytes, x: int, y: int) -> tuple[int, int, int]:
    offset = (y * WIDTH + x) * 4
    blue, green, red = raw[offset : offset + 3]
    return red, green, blue


def labelled_runs(
    raw: bytes,
    y: int,
    guard: tuple[int, int, int],
    marker: tuple[int, int, int],
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
            raise ValueError(f"row {y} sample {x}: unexpected RGB {rgb}")
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


def expected_runs(
    profile: str,
    case_id: str,
    role: str,
) -> list[tuple[str, int, int]]:
    marker_start = 370 if profile == "ecs" else 371
    if role == "pre-mutation baseline":
        if case_id == "midline-hbstrt-past":
            return [
                ("guard", 2, 520),
                ("blank", 520, 584),
                ("guard", 584, 756),
            ]
        if case_id == "midline-hbstop-future":
            return [
                ("guard", 2, 200),
                ("blank", 200, 264),
                ("guard", 264, 756),
            ]
        if profile == "aga" and case_id == "midline-blanken-enable":
            return [
                ("guard", 2, 264),
                ("blank", 264, 520),
                ("guard", 520, 756),
            ]
        return [("guard", 2, 756)]

    if role == "mutation output":
        if case_id == "midline-hbstrt-past":
            return [
                ("guard", 2, marker_start),
                ("marker", marker_start, 756),
            ]
        if case_id == "midline-hbstop-future":
            return [
                ("guard", 2, 200),
                ("blank", 200, 264),
                ("guard", 264, marker_start),
                ("marker", marker_start, 756),
            ]
        if case_id in {"midline-ecsena-enable", "midline-extblken-enable"}:
            if profile == "ecs":
                return [
                    ("guard", 2, marker_start),
                    ("marker", marker_start, 390),
                    ("blank", 390, 520),
                    ("marker", 520, 756),
                ]
            return [
                ("guard", 2, marker_start),
                ("marker", marker_start, 756),
            ]
        if profile == "aga":
            return [
                ("guard", 2, 264),
                ("blank", 264, 520),
                ("marker", 520, 756),
            ]
        return [
            ("guard", 2, marker_start),
            ("marker", marker_start, 756),
        ]

    if role != "post-mutation control":
        raise ValueError(f"unknown line role: {role}")
    if case_id == "midline-hbstrt-past":
        return [
            ("marker", 2, 264),
            ("blank", 264, 584),
            ("marker", 584, 756),
        ]
    if case_id == "midline-hbstop-future":
        return [
            ("marker", 2, 200),
            ("blank", 200, 520),
            ("marker", 520, 756),
        ]
    return [
        ("marker", 2, 264),
        ("blank", 264, 520),
        ("marker", 520, 756),
    ]


def find_observed_rows(
    raw: bytes,
    profile: str,
    case: dict[str, Any],
) -> tuple[int, list[dict[str, Any]]]:
    guard = rgb4(case["identity"]["visual"]["color00"])
    marker = rgb4(case["identity"]["visual"]["marker_color00"])
    candidates: list[int] = []
    for y in range(2, HEIGHT - 2):
        row_rgbs = {
            pixel_rgb(raw, x, y)
            for x in range(STORAGE_EXCLUSION[1], WIDTH)
        }
        if guard in row_rgbs and marker in row_rgbs:
            candidates.append(y)
    if len(candidates) != 2 or candidates[1] != candidates[0] + 1:
        raise ValueError(
            f"{profile}/{case['id']}: marker rows are {candidates}, expected one doubled pair"
        )
    mutation_row = candidates[0]

    roles_and_rows = [
        ("pre-mutation baseline", mutation_row - 2),
        ("mutation output", mutation_row),
        ("post-mutation control", mutation_row + 2),
    ]
    observations: list[dict[str, Any]] = []
    row_bytes = WIDTH * 4
    for role, y in roles_and_rows:
        first = raw[y * row_bytes : (y + 1) * row_bytes]
        second = raw[(y + 1) * row_bytes : (y + 2) * row_bytes]
        require_equal(second, first, f"{profile}/{case['id']}: doubled rows {y}/{y + 1}")
        for storage_x in range(*STORAGE_EXCLUSION):
            require_equal(
                pixel_rgb(raw, storage_x, y),
                (0, 0, 0),
                f"{profile}/{case['id']}: storage sample {storage_x} row {y}",
            )
        runs = labelled_runs(raw, y, guard, marker)
        require_equal(
            runs,
            expected_runs(profile, case["id"], role),
            f"{profile}/{case['id']}: {role}",
        )
        observations.append(
            {
                "role": role,
                "raw_rows": [y, y + 1],
                "black_runs": [[start, stop] for label, start, stop in runs if label == "blank"],
                "guard_runs": [[start, stop] for label, start, stop in runs if label == "guard"],
                "marker_runs": [[start, stop] for label, start, stop in runs if label == "marker"],
            }
        )
    return mutation_row, observations


def validate_input_manifests(
    run_dir: Path,
    manifest: dict[str, Any],
    artifact: dict[str, Any],
) -> None:
    before_path = run_dir / "inputs-before.sha256"
    after_path = run_dir / "inputs-after.sha256"
    require_equal(
        before_path.read_bytes(),
        after_path.read_bytes(),
        f"{run_dir}: before/after input manifests",
    )
    inputs = manifest["inputs"]
    require_equal(
        sha256_file(before_path),
        inputs["before_manifest_sha256"],
        f"{before_path}: SHA-256",
    )
    require_equal(
        sha256_file(after_path),
        inputs["after_manifest_sha256"],
        f"{after_path}: SHA-256",
    )
    require_equal(inputs["unchanged_during_capture"], True, f"{run_dir}: inputs")
    expected = {
        Path(inputs["firmware"]["file"]).name: PROFILES[
            manifest["capture"]["profile"]
        ]["firmware_sha256"],
        artifact["adf_file"]: artifact["sha256"]["adf"],
        artifact["payload_file"]: artifact["sha256"]["payload"],
        "suite-v1.json": SUITE_SHA256,
        "config.uae": inputs["configuration"]["sha256"],
        "fs-uae": PRODUCER["binary_sha256"],
        **CAPTURE_TOOLS,
    }
    require_equal(
        manifest_by_basename(before_path),
        expected,
        f"{before_path}: recorded inputs",
    )


def validate_capture_manifest(
    run_dir: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    suite_dir: Path,
) -> dict[str, Any]:
    path = run_dir / "capture-manifest.json"
    manifest = read_json(path)
    require_equal(manifest.get("schema_version"), "1.0.0", f"{path}: schema")

    producer = manifest["producer"]
    for key, expected in {
        "product": PRODUCER["product"],
        "version": PRODUCER["version"],
        "revision": PRODUCER["revision"],
        "source_url": PRODUCER["source_url"],
        "uae_base_version": PRODUCER["uae_base_version"],
        "binary_sha256": PRODUCER["binary_sha256"],
        "capture_patch_sha256": PRODUCER["capture_patch_sha256"],
    }.items():
        require_equal(producer[key], expected, f"{path}: producer {key}")
    require_equal(
        sha256_file(Path(producer["binary_file"])),
        PRODUCER["binary_sha256"],
        f"{path}: producer binary",
    )

    tools = manifest["capture_tools"]
    for name, expected in CAPTURE_TOOLS.items():
        require_equal(tools[name], expected, f"{path}: tool {name}")
        require_equal(
            sha256_file(Path(tools["directory"]) / name),
            expected,
            f"{path}: tool file {name}",
        )

    capture = manifest["capture"]
    require_equal(capture["profile"], profile, f"{path}: profile")
    require_equal(capture["case_id"], case["id"], f"{path}: case")
    if not capture["operator"].strip() or not capture["host"].strip():
        raise ValueError(f"{path}: operator or host is empty")
    if TIMESTAMP_RE.fullmatch(capture["captured_at_utc"]) is None:
        raise ValueError(f"{path}: invalid capture timestamp")

    expected_suite = {
        "id": SUITE_ID,
        "version": SUITE_VERSION,
        "source_revision": SUITE_SOURCE_REVISION,
        "manifest_sha256": SUITE_SHA256,
        "case_id": case["id"],
        "numeric_id": case["numeric_id"],
        "adf_file": artifact["adf_file"],
        "adf_sha256": artifact["sha256"]["adf"],
        "payload_file": artifact["payload_file"],
        "payload_sha256": artifact["sha256"]["payload"],
    }
    require_equal(manifest["suite"], expected_suite, f"{path}: suite")

    inputs = manifest["inputs"]
    expected_files = {
        "adf": run_dir / "inputs" / artifact["adf_file"],
        "payload": run_dir / "inputs" / artifact["payload_file"],
        "suite_manifest": run_dir / "inputs" / "suite-v1.json",
        "configuration": run_dir / "config.uae",
    }
    expected_hashes = {
        "adf": artifact["sha256"]["adf"],
        "payload": artifact["sha256"]["payload"],
        "suite_manifest": SUITE_SHA256,
        "configuration": sha256_file(run_dir / "config.uae"),
    }
    for name, expected_path in expected_files.items():
        record = inputs[name]
        require_equal(
            Path(record["file"]).resolve(),
            expected_path.resolve(),
            f"{path}: {name} path",
        )
        require_equal(
            sha256_file(expected_path),
            expected_hashes[name],
            f"{path}: {name} file",
        )
        require_equal(record["sha256"], expected_hashes[name], f"{path}: {name} hash")
        if name in {"adf", "payload"}:
            require_equal(record["mode"], "0o444", f"{path}: {name} mode")
            require_equal(
                stat.S_IMODE(expected_path.stat().st_mode),
                0o444,
                f"{expected_path}: mode",
            )

    for name in ("adf", "payload", "suite_manifest"):
        source_name = (
            artifact["adf_file"]
            if name == "adf"
            else artifact["payload_file"]
            if name == "payload"
            else "suite-v1.json"
        )
        require_equal(
            sha256_file(suite_dir / source_name),
            sha256_file(expected_files[name]),
            f"{run_dir}: staged {name}",
        )

    firmware = inputs["firmware"]
    require_equal(
        firmware["sha256"],
        PROFILES[profile]["firmware_sha256"],
        f"{path}: firmware identity",
    )
    require_equal(
        sha256_file(Path(firmware["file"])),
        firmware["sha256"],
        f"{path}: firmware file",
    )
    validate_config(run_dir / "config.uae", profile, case["id"], manifest)
    validate_input_manifests(run_dir, manifest, artifact)
    require_equal(
        sha256_file(run_dir / "run.stdout"),
        manifest["files"]["run_log_sha256"],
        f"{path}: run log",
    )
    require_equal(
        sha256_file(run_dir / "capture.sha256"),
        manifest["files"]["capture_hash_manifest_sha256"],
        f"{path}: capture hash manifest",
    )
    return manifest


def load_run(
    capture_root: Path,
    suite_dir: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
) -> tuple[list[Image.Image], dict[str, Any], int, list[dict[str, Any]]]:
    run_dir = capture_root / profile / case["id"]
    manifest = validate_capture_manifest(
        run_dir, profile, case, artifact, suite_dir
    )
    readiness = manifest["readiness"]
    core_fields = readiness["captured_core_fields"]
    guest_fields = readiness["captured_guest_field_counters"]
    require_equal(len(core_fields), FRAME_COUNT, f"{run_dir}: core fields")
    require_equal(
        core_fields,
        list(range(core_fields[0], core_fields[0] + FRAME_COUNT)),
        f"{run_dir}: adjacent core fields",
    )
    require_equal(guest_fields, GUEST_FIELDS, f"{run_dir}: guest fields")
    require_equal(readiness["ready_guest_field_counter"], 1, f"{run_dir}: ready")
    require_equal(readiness["settle_fields"], SETTLE_FIELDS, f"{run_dir}: settle")
    require_equal(
        readiness["ready_core_field"] + SETTLE_FIELDS,
        core_fields[0],
        f"{run_dir}: core settle interval",
    )

    raw_capture = manifest["raw_capture"]
    expected_geometry = {
        "width": WIDTH,
        "height": HEIGHT,
        "packed_stride_bytes": PACKED_STRIDE,
        "producer_stride_bytes": PRODUCER_STRIDE,
        "pixel_format": "BGRA8888",
        "adjacent_field_stability": "byte-identical",
    }
    require_equal(
        {key: raw_capture[key] for key in expected_geometry},
        expected_geometry,
        f"{run_dir}: geometry",
    )

    field_records = raw_capture["fields"]
    require_equal(len(field_records), FRAME_COUNT, f"{run_dir}: raw fields")
    capture_hashes = manifest_by_basename(run_dir / "capture.sha256")
    expected_capture_files: dict[str, str] = {}
    raw_frames: list[bytes] = []
    ready = expected_ready(case)
    for index, field_record in enumerate(field_records):
        core_field = core_fields[index]
        guest_field = guest_fields[index]
        raw_path = run_dir / "capture" / field_record["raw_file"]
        metadata_path = run_dir / "capture" / field_record["metadata_file"]
        require_equal(
            raw_path.name,
            f"field-{core_field:06d}.bgra",
            f"{run_dir}: raw name",
        )
        require_equal(
            metadata_path.name,
            f"field-{core_field:06d}.json",
            f"{run_dir}: metadata name",
        )
        raw = raw_path.read_bytes()
        require_equal(len(raw), WIDTH * HEIGHT * 4, f"{raw_path}: bytes")
        require_equal(
            sha256_bytes(raw),
            field_record["raw_sha256"],
            f"{raw_path}: hash",
        )
        metadata = read_json(metadata_path)
        require_equal(
            sha256_file(metadata_path),
            field_record["metadata_sha256"],
            f"{metadata_path}: hash",
        )
        require_equal(metadata, field_record["metadata"], f"{metadata_path}: embedded")
        require_equal(
            metadata,
            {
                "schema": "org.198x.fs-uae.raw-capture/v1",
                "core_field": core_field,
                "guest_field_counter": guest_field,
                "ready": ready,
                "framebuffer": FRAMEBUFFER,
                "frontend_compatibility_view": FRONTEND_VIEW,
            },
            f"{metadata_path}: content",
        )
        expected_capture_files[raw_path.name] = field_record["raw_sha256"]
        expected_capture_files[metadata_path.name] = field_record["metadata_sha256"]
        raw_frames.append(raw)

    require_equal(capture_hashes, expected_capture_files, f"{run_dir}: files")
    if len(set(raw_frames)) != 1:
        raise ValueError(f"{run_dir}: adjacent raw fields are not byte-identical")

    mutation_row, observations = find_observed_rows(raw_frames[0], profile, case)
    for raw in raw_frames[1:]:
        require_equal(
            find_observed_rows(raw, profile, case),
            (mutation_row, observations),
            f"{run_dir}: observed rows across fields",
        )

    log_text = (run_dir / "run.stdout").read_text(encoding="utf-8")
    marker = (
        f"CODEX_CAPTURE complete first_core_field={core_fields[0]} "
        f"last_core_field={core_fields[-1]} first_guest_field={guest_fields[0]} "
        f"last_guest_field={guest_fields[-1]}"
    )
    if marker not in log_text or "CODEX_CAPTURE error=" in log_text:
        raise ValueError(f"{run_dir}: completion log is invalid")

    frames = [
        Image.frombytes("RGBA", (WIDTH, HEIGHT), raw, "raw", "BGRA")
        for raw in raw_frames
    ]
    return frames, manifest, mutation_row, observations


def png_chunk(chunk_type: bytes, payload: bytes) -> bytes:
    checksum = binascii.crc32(chunk_type + payload) & 0xFFFF_FFFF
    return (
        struct.pack(">I", len(payload))
        + chunk_type
        + payload
        + struct.pack(">I", checksum)
    )


def write_apng(path: Path, frames: list[Image.Image]) -> str:
    def compressed_frame(image: Image.Image) -> bytes:
        rgba = image.tobytes()
        filtered = b"".join(
            b"\0" + rgba[row * PACKED_STRIDE : (row + 1) * PACKED_STRIDE]
            for row in range(HEIGHT)
        )
        return zlib.compress(filtered, level=9)

    chunks = [
        b"\x89PNG\r\n\x1a\n",
        png_chunk(b"IHDR", struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 6, 0, 0, 0)),
        png_chunk(b"acTL", struct.pack(">II", len(frames), 0)),
    ]
    sequence = 0
    for index, frame in enumerate(frames):
        chunks.append(
            png_chunk(
                b"fcTL",
                struct.pack(
                    ">IIIIIHHBB",
                    sequence,
                    WIDTH,
                    HEIGHT,
                    0,
                    0,
                    1,
                    50,
                    0,
                    0,
                ),
            )
        )
        sequence += 1
        compressed = compressed_frame(frame)
        if index == 0:
            chunks.append(png_chunk(b"IDAT", compressed))
        else:
            chunks.append(
                png_chunk(b"fdAT", struct.pack(">I", sequence) + compressed)
            )
            sequence += 1
    chunks.append(png_chunk(b"IEND", b""))
    path.write_bytes(b"".join(chunks))

    with Image.open(path) as packaged:
        require_equal(packaged.n_frames, len(frames), f"{path}: frame count")
        decoded = [
            frame.convert("RGBA").tobytes()
            for frame in ImageSequence.Iterator(packaged)
        ]
    original = [frame.tobytes() for frame in frames]
    require_equal(decoded, original, f"{path}: decode round trip")
    return sha256_bytes(b"".join(decoded))


def intervals(
    lines: list[dict[str, Any]],
    role: str,
    kind: str,
) -> list[list[int]]:
    return next(line[kind] for line in lines if line["role"] == role)


def make_record(
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    manifest: dict[str, Any],
    mutation_row: int,
    lines: list[dict[str, Any]],
    files: dict[str, tuple[str, str]],
) -> dict[str, Any]:
    profile_data = PROFILES[profile]
    timed = case["timed_write"]
    register_key = timed["register"].lower()
    marker_runs = intervals(lines, "mutation output", "marker_runs")
    marker_start = marker_runs[0][0]
    core_fields = manifest["readiness"]["captured_core_fields"]
    config_name, config_sha = files["config"]
    log_name, log_sha = files["log"]
    manifest_name, manifest_sha = files["manifest"]
    apng_name, apng_sha = files["capture"]
    decoded_sha = files["decoded"][1]

    return {
        "schema_version": "1.0.0",
        "suite_id": SUITE_ID,
        "suite_version": SUITE_VERSION,
        "case_id": case["id"],
        "artifact": {
            "adf_file": artifact["adf_file"],
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": artifact["payload_file"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "kind": "software-emulator",
            "product": PRODUCER["product"],
            "version": PRODUCER["version"],
            "revision": PRODUCER["revision"],
            "source_url": PRODUCER["source_url"],
            "implementation_family": "UAE",
        },
        "machine": {
            "model": profile_data["model"],
            "cpu": profile_data["cpu"],
            "agnus": profile_data["agnus"],
            "denise_or_lisa": profile_data["denise_or_lisa"],
            "chipset": profile_data["chipset"],
            "region": "PAL",
            "ram_bytes": profile_data["ram_bytes"],
            "firmware": {
                "revision": profile_data["firmware_revision"],
                "sha256": profile_data["firmware_sha256"],
            },
        },
        "execution": {
            "cold_boot": True,
            "command_or_procedure": (
                "tools/fs-uae-hblank-write-timing-capture/capture.sh "
                f"{profile} {case['id']} "
                "<registered-FS-UAE-binary> <suite-1.0.0-directory> "
                "<firmware-matching-recorded-sha256> "
                "<fresh-output-directory> <operator>"
            ),
            "configuration_sha256": config_sha,
            "ready_rule": {
                "record_address": "0x0002ff00",
                "magic": "HBLK",
                "case_number": case["numeric_id"],
                "field_counter_minimum": 8,
                "byte_order": "big-endian",
            },
            "ready_observed_field": manifest["readiness"]["ready_core_field"],
            "settle_fields": SETTLE_FIELDS,
            "captured_fields": core_fields,
            "adjacent_field_stability": "confirmed",
        },
        "source_capture": {
            "method": (
                "FS-UAE raw UAE-core framebuffer hook, losslessly reordered "
                "from BGRA and packaged as APNG"
            ),
            "width": WIDTH,
            "height": HEIGHT,
            "pixel_format": (
                "RGBA8888, tightly packed, row-major; alpha retained from "
                "source BGRA8888"
            ),
            "stride_bytes": PACKED_STRIDE,
            "blanking_retained": True,
            "overscan_retained": True,
            "filter": "none",
            "scaling": "none",
            "shader": "none",
            "file_name": f"../captures/{apng_name}",
            "file_sha256": apng_sha,
            "decoded_pixel_sha256": decoded_sha,
        },
        "normalization": {
            "beam_coordinate": {
                "sample_beam_line": timed["beam_line"],
                "sample_row": mutation_row,
                "horizontal_origin_sample": -184,
                "horizontal_samples_per_register_increment_numerator": 4,
                "horizontal_samples_per_register_increment_denominator": 1,
                "phase_numerator": 0,
                "phase_denominator": 1,
            },
            "crop": {"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT},
            "field_handling": "bob",
            "color_conversion": (
                "lossless BGRA-to-RGBA channel reorder; black classified by "
                "RGB regardless of retained source alpha"
            ),
            "alignment_search": False,
        },
        "stimulus": {
            "reset_beam_line": timed["reset_beam_line"],
            "reset_wait_hpos_cck": timed["reset_hpos_cck"],
            "mutation_beam_line": timed["beam_line"],
            "mutation_wait_hpos_cck": timed["wait_hpos_cck"],
            "tested_register": timed["register"],
            "baseline_word": case["registers"][register_key]["word"],
            "mutation_word": timed["word"],
            "write_position_evidence": {
                "method": (
                    "Copper schedule with immediately preceding visible "
                    "COLOR00 marker"
                ),
                "marker_start_sample": marker_start,
                "tested_write_sample": None,
                "uncertainty": (
                    "The marker proves the preceding MOVE and bounds schedule "
                    "order. This framebuffer does not directly expose the "
                    "tested register bus-write sample."
                ),
            },
        },
        "observations": {
            "guard_color_word": case["identity"]["visual"]["color00"],
            "marker_color_word": case["identity"]["visual"]["marker_color00"],
            "storage_exclusion": list(STORAGE_EXCLUSION),
            "interval_convention": "start-inclusive-stop-exclusive",
            "lines": lines,
            "following_line_carry": "mutation confirmed",
            "uncertainty_samples": 0,
            "notes": [
                (
                    f"The marked mutation output was discovered at raw rows "
                    f"{mutation_row}/{mutation_row + 1}; it was not assumed "
                    "to be the steady-state suite's row 202."
                ),
                (
                    "The raw x=[0, 2) transparent-black storage boundary is "
                    "retained in the APNG and excluded from semantic runs."
                ),
                (
                    f"Ready was observed at core field "
                    f"{manifest['readiness']['ready_core_field']}; core fields "
                    f"{core_fields} and guest counters {GUEST_FIELDS} were "
                    "captured after eight settling fields."
                ),
                (
                    f"Configuration ../configs/{config_name} has SHA-256 "
                    f"{config_sha}."
                ),
                f"Run log ../logs/{log_name} has SHA-256 {log_sha}.",
                (
                    f"Capture manifest ../manifests/{manifest_name} has "
                    f"SHA-256 {manifest_sha}."
                ),
            ],
        },
        "provenance": {
            "operator": manifest["capture"]["operator"],
            "capture_date": manifest["capture"]["captured_at_utc"][:10],
            "host": manifest["capture"]["host"],
            "classification": "software-derived",
        },
    }


def package(capture_root: Path, suite_dir: Path, output_root: Path) -> None:
    suite, suite_sha = validate_suite(suite_dir)
    validate_topology(capture_root)
    producer_build = output_root / "producer-build-v1.json"
    require_equal(
        sha256_file(producer_build),
        PRODUCER_BUILD_SHA256,
        f"{producer_build}: SHA-256",
    )
    cases = {case["id"]: case for case in suite["cases"]}
    artifacts = {
        artifact["case_id"]: artifact for artifact in suite["artifacts"]
    }

    output_dirs = {
        name: output_root / name
        for name in ("captures", "records", "logs", "configs", "manifests")
    }
    for directory in output_dirs.values():
        directory.mkdir(parents=True, exist_ok=True)

    packaged_runs: list[dict[str, Any]] = []
    configurations: dict[str, str] = {}
    for profile in PROFILES:
        for case_id in CASE_IDS:
            case = cases[case_id]
            artifact = artifacts[case_id]
            frames, manifest, mutation_row, lines = load_run(
                capture_root, suite_dir, profile, case, artifact
            )
            run_dir = capture_root / profile / case_id
            stem = f"{profile}--{case_id}"

            config_path = output_dirs["configs"] / f"{stem}.uae"
            log_path = output_dirs["logs"] / f"{stem}.log"
            manifest_path = output_dirs["manifests"] / f"{stem}.json"
            shutil.copyfile(run_dir / "config.uae", config_path)
            shutil.copyfile(run_dir / "run.stdout", log_path)
            shutil.copyfile(run_dir / "capture-manifest.json", manifest_path)

            capture_path = output_dirs["captures"] / f"{stem}.apng"
            decoded_sha = write_apng(capture_path, frames)
            files = {
                "config": (config_path.name, sha256_file(config_path)),
                "log": (log_path.name, sha256_file(log_path)),
                "manifest": (manifest_path.name, sha256_file(manifest_path)),
                "capture": (capture_path.name, sha256_file(capture_path)),
                "decoded": ("concatenated decoded RGBA", decoded_sha),
            }
            configurations[stem] = files["config"][1]

            record = make_record(
                profile,
                case,
                artifact,
                manifest,
                mutation_row,
                lines,
                files,
            )
            record_path = output_dirs["records"] / f"{stem}.json"
            record_path.write_text(
                json.dumps(record, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            packaged_runs.append(
                {
                    "profile": profile,
                    "case_id": case_id,
                    "mutation_output_rows": [mutation_row, mutation_row + 1],
                    "configuration_file": f"configs/{config_path.name}",
                    "configuration_sha256": files["config"][1],
                    "capture_manifest_file": f"manifests/{manifest_path.name}",
                    "capture_manifest_sha256": files["manifest"][1],
                    "run_log_file": f"logs/{log_path.name}",
                    "run_log_sha256": files["log"][1],
                    "capture_file": f"captures/{capture_path.name}",
                    "capture_sha256": files["capture"][1],
                    "decoded_pixel_sha256": decoded_sha,
                    "record_file": f"records/{record_path.name}",
                    "record_sha256": sha256_file(record_path),
                }
            )
            print(stem)

    expected_stems = {
        f"{profile}--{case_id}"
        for profile in PROFILES
        for case_id in CASE_IDS
    }
    for name, suffix in (
        ("captures", ".apng"),
        ("records", ".json"),
        ("logs", ".log"),
        ("configs", ".uae"),
        ("manifests", ".json"),
    ):
        actual = {path.stem for path in output_dirs[name].glob(f"*{suffix}")}
        require_equal(actual, expected_stems, f"{output_dirs[name]}: files")

    package_manifest = {
        "schema_version": "1.0.0",
        "suite": {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": SUITE_SOURCE_REVISION,
            "manifest_sha256": suite_sha,
        },
        "producer": {
            **PRODUCER,
            "build_manifest_file": "producer-build-v1.json",
            "build_manifest_sha256": PRODUCER_BUILD_SHA256,
        },
        "capture_tools": CAPTURE_TOOLS,
        "matrix": {
            "profiles": list(PROFILES),
            "cases": list(CASE_IDS),
            "run_count": len(expected_stems),
            "raw_width": WIDTH,
            "raw_height": HEIGHT,
            "raw_pixel_format": "BGRA8888",
            "packaged_pixel_format": "RGBA8888",
        },
        "configurations": configurations,
        "packager": {
            "script_sha256": sha256_file(Path(__file__).resolve()),
            "python_implementation": sys.implementation.name,
            "python_version": sys.version.split()[0],
            "pillow_version": PIL.__version__,
            "zlib_build_version": zlib.ZLIB_VERSION,
            "zlib_runtime_version": zlib.ZLIB_RUNTIME_VERSION,
        },
        "runs": packaged_runs,
    }
    (output_root / "package-v1.json").write_text(
        json.dumps(package_manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture_root", type=Path)
    parser.add_argument("suite_dir", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(__file__).resolve().parent,
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        package(
            args.capture_root.resolve(),
            args.suite_dir.resolve(),
            args.output.resolve(),
        )
    except (
        KeyError,
        OSError,
        TypeError,
        ValueError,
        json.JSONDecodeError,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
