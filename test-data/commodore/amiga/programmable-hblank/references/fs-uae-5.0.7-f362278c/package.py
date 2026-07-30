#!/usr/bin/env python3
"""Verify the FS-UAE v2 programmable-HBLANK matrix and package its records."""

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


SUITE_ID = "org.198x.amiga.programmable-hblank"
SUITE_VERSION = "1.0.1"
SUITE_SOURCE_REVISION = "source-v2"
SUITE_SHA256 = "f8f70818fb0a7454db283deb48b75858302bece28922f3b1f2dfab0d59503b24"

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
    "capture.sh": "2eed1317f5764f326fb7a2a71ec5744f2ab488a1e6acdd579d8c6e04650878a5",
    "capture_manifest.py": (
        "fad86749afe5867e3ea1bc5417244eefdfcfe9e70bae5bac1cd810985346a922"
    ),
    "config.uae.in": (
        "fe7f7c706a97c6a7533b2d3760e9dc855dfa3811e611a5a0b840e3e1eaabc699"
    ),
}

WIDTH = 756
HEIGHT = 576
PACKED_STRIDE = WIDTH * 4
PRODUCER_STRIDE = 8192
SAMPLE_ROW = 202
SAMPLE_BEAM_LINE = 128
FRAME_COUNT = 3
GUEST_FIELDS = [9, 10, 11]
SETTLE_FIELDS = 8

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

PROFILES = {
    "ecs": {
        "case_ids": [
            "fixed-control",
            "ecsena-gate",
            "extblken-gate",
            "blanken-path",
            "programmed-central",
            "programmed-wrap",
            "programmed-equal",
        ],
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
        "case_ids": [
            "fixed-control",
            "ecsena-gate",
            "extblken-gate",
            "blanken-path",
            "programmed-central",
            "programmed-wrap",
            "programmed-equal",
            "aga-fine-lores",
            "aga-fine-hires",
            "aga-fine-shres",
        ],
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

CASE_IDS = [
    "fixed-control",
    "ecsena-gate",
    "extblken-gate",
    "blanken-path",
    "programmed-central",
    "programmed-wrap",
    "programmed-equal",
    "aga-fine-lores",
    "aga-fine-hires",
    "aga-fine-shres",
]

# These include the raw producer's two-sample transparent-black boundary
# invariant at x=[0, 2). Semantic programmable edges are kept separately.
EXPECTED_BLACK_RUNS = {
    ("ecs", "fixed-control"): [(0, 2)],
    ("ecs", "ecsena-gate"): [(0, 2)],
    ("ecs", "extblken-gate"): [(0, 2)],
    ("ecs", "blanken-path"): [(0, 2)],
    ("ecs", "programmed-central"): [(0, 2), (328, 456)],
    ("ecs", "programmed-wrap"): [(0, 72), (648, 756)],
    ("ecs", "programmed-equal"): [(0, 2)],
    ("aga", "fixed-control"): [(0, 2)],
    ("aga", "ecsena-gate"): [(0, 2)],
    ("aga", "extblken-gate"): [(0, 2)],
    ("aga", "blanken-path"): [(0, 2), (328, 456)],
    ("aga", "programmed-central"): [(0, 2), (328, 456)],
    ("aga", "programmed-wrap"): [(0, 72), (648, 756)],
    ("aga", "programmed-equal"): [(0, 2)],
    ("aga", "aga-fine-lores"): [(0, 2), (328, 459)],
    ("aga", "aga-fine-hires"): [(0, 2), (328, 459)],
    ("aga", "aga-fine-shres"): [(0, 2), (328, 459)],
}

SEMANTIC_EDGES = {
    ("ecs", "fixed-control"): (None, None),
    ("ecs", "ecsena-gate"): (None, None),
    ("ecs", "extblken-gate"): (None, None),
    ("ecs", "blanken-path"): (None, None),
    ("ecs", "programmed-central"): (328, 456),
    ("ecs", "programmed-wrap"): (648, 72),
    ("ecs", "programmed-equal"): (None, None),
    ("aga", "fixed-control"): (None, None),
    ("aga", "ecsena-gate"): (None, None),
    ("aga", "extblken-gate"): (None, None),
    ("aga", "blanken-path"): (328, 456),
    ("aga", "programmed-central"): (328, 456),
    ("aga", "programmed-wrap"): (648, 72),
    ("aga", "programmed-equal"): (None, None),
    ("aga", "aga-fine-lores"): (328, 459),
    ("aga", "aga-fine-hires"): (328, 459),
    ("aga", "aga-fine-shres"): (328, 459),
}

TIMESTAMP_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\+00:00$"
)
SHA_LINE_RE = re.compile(r"^([0-9a-f]{64})  (.+)$")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


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
    require_equal([case["id"] for case in cases], CASE_IDS, f"{suite_path}: cases")
    require_equal(
        [artifact["case_id"] for artifact in artifacts],
        CASE_IDS,
        f"{suite_path}: artifacts",
    )

    for case, artifact in zip(cases, artifacts, strict=True):
        if case["numeric_id"] != CASE_IDS.index(case["id"]) + 1:
            raise ValueError(f"{suite_path}: invalid numeric ID for {case['id']}")
        for kind, file_key, bytes_key in (
            ("ADF", "adf_file", "adf_bytes"),
            ("payload", "payload_file", "payload_bytes"),
        ):
            artifact_path = suite_dir / artifact[file_key]
            require_equal(
                artifact_path.stat().st_size,
                artifact[bytes_key],
                f"{artifact_path}: byte count",
            )
            require_equal(
                sha256_file(artifact_path),
                artifact["sha256"]["adf" if kind == "ADF" else "payload"],
                f"{artifact_path}: SHA-256",
            )
    return suite, suite_sha256


def validate_topology(capture_root: Path) -> None:
    actual_profiles = {
        path.name for path in capture_root.iterdir() if not path.name.startswith(".")
    }
    require_equal(actual_profiles, set(PROFILES), f"{capture_root}: profiles")
    for profile, profile_data in PROFILES.items():
        profile_dir = capture_root / profile
        if not profile_dir.is_dir():
            raise ValueError(f"{profile_dir}: expected a directory")
        actual_cases = {
            path.name for path in profile_dir.iterdir() if not path.name.startswith(".")
        }
        require_equal(
            actual_cases,
            set(profile_data["case_ids"]),
            f"{profile_dir}: cases",
        )
        for case_id in actual_cases:
            if not (profile_dir / case_id).is_dir():
                raise ValueError(f"{profile_dir / case_id}: expected a directory")


def validate_config(
    path: Path,
    profile: str,
    case_id: str,
    manifest: dict[str, Any],
) -> None:
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

    expected_common = {
        "config_description": f"198x programmable-HBLANK {profile} {case_id}",
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
    expected_common.update(PROFILES[profile]["config"])
    for key, expected in expected_common.items():
        require_equal(values.get(key), expected, f"{path}: {key}")

    require_equal(
        Path(values["kickstart_rom_file"]).resolve(),
        Path(manifest["inputs"]["firmware"]["file"]).resolve(),
        f"{path}: kickstart path",
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
        # The capture hook deliberately records the fixed 32-byte identity area.
        "identity": case["identity"]["serial"]["value"][:32],
        "bplcon0": f"0x{int(registers['bplcon0']['word'], 16):04x}",
        "bplcon3": f"0x{int(registers['bplcon3']['word'], 16):04x}",
        "beamcon0": f"0x{int(registers['beamcon0']['word'], 16):04x}",
        "hbstrt": f"0x{int(registers['hbstrt']['word'], 16):04x}",
        "hbstop": f"0x{int(registers['hbstop']['word'], 16):04x}",
        "color00": f"0x{int(case['line_geometry']['guard_color_word'], 16):04x}",
    }


def guard_bgra(case: dict[str, Any]) -> bytes:
    word = int(case["line_geometry"]["guard_color_word"], 16)
    red = ((word >> 8) & 0xF) * 17
    green = ((word >> 4) & 0xF) * 17
    blue = (word & 0xF) * 17
    return bytes((blue, green, red, 255))


def black_runs(image: Image.Image) -> list[tuple[int, int]]:
    pixels = image.convert("RGBA")
    row = [pixels.getpixel((x, SAMPLE_ROW))[:3] == (0, 0, 0) for x in range(WIDTH)]
    runs: list[tuple[int, int]] = []
    start: int | None = None
    for sample, is_black in enumerate([*row, False]):
        if is_black and start is None:
            start = sample
        elif not is_black and start is not None:
            runs.append((start, sample))
            start = None
    return runs


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
    require_equal(
        inputs["unchanged_during_capture"],
        True,
        f"{run_dir}: input immutability",
    )

    expected_hashes = {
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
        expected_hashes,
        f"{before_path}: recorded inputs",
    )


def validate_capture_manifest(
    run_dir: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    suite_dir: Path,
    suite_sha256: str,
) -> dict[str, Any]:
    manifest_path = run_dir / "capture-manifest.json"
    manifest = read_json(manifest_path)
    require_equal(manifest.get("schema_version"), "1.0.0", f"{manifest_path}: schema")

    expected_producer = {
        "product": PRODUCER["product"],
        "version": PRODUCER["version"],
        "revision": PRODUCER["revision"],
        "source_url": PRODUCER["source_url"],
        "uae_base_version": PRODUCER["uae_base_version"],
        "binary_sha256": PRODUCER["binary_sha256"],
        "capture_patch_sha256": PRODUCER["capture_patch_sha256"],
    }
    producer = manifest["producer"]
    require_equal(
        {key: producer[key] for key in expected_producer},
        expected_producer,
        f"{manifest_path}: producer",
    )
    binary_path = Path(producer["binary_file"])
    require_equal(
        sha256_file(binary_path),
        PRODUCER["binary_sha256"],
        f"{binary_path}: producer binary SHA-256",
    )

    capture_tools = manifest["capture_tools"]
    require_equal(
        {key: capture_tools[key] for key in CAPTURE_TOOLS},
        CAPTURE_TOOLS,
        f"{manifest_path}: capture tools",
    )
    tool_dir = Path(capture_tools["directory"])
    for name, expected_sha256 in CAPTURE_TOOLS.items():
        require_equal(
            sha256_file(tool_dir / name),
            expected_sha256,
            f"{tool_dir / name}: SHA-256",
        )

    capture = manifest["capture"]
    require_equal(capture["profile"], profile, f"{manifest_path}: profile")
    require_equal(capture["case_id"], case["id"], f"{manifest_path}: case")
    if not capture["operator"].strip() or not capture["host"].strip():
        raise ValueError(f"{manifest_path}: operator or host is empty")
    if TIMESTAMP_RE.fullmatch(capture["captured_at_utc"]) is None:
        raise ValueError(f"{manifest_path}: invalid UTC capture timestamp")
    expected_command = [str(binary_path), str((run_dir / "config.uae").resolve())]
    require_equal(capture["command"], expected_command, f"{manifest_path}: command")
    expected_environment = {
        "FSEMU_CODEX_CAPTURE_DIR": str((run_dir / "capture").resolve()),
        "FSEMU_CODEX_CAPTURE_CASE_NUMBER": str(case["numeric_id"]),
        "FSEMU_CODEX_CAPTURE_MIN_FIELD_COUNTER": "9",
        "FSEMU_QUIT_AFTER_N_FRAMES": "600",
    }
    require_equal(
        capture["environment"],
        expected_environment,
        f"{manifest_path}: environment",
    )

    expected_suite = {
        "id": SUITE_ID,
        "version": SUITE_VERSION,
        "source_revision": SUITE_SOURCE_REVISION,
        "manifest_sha256": suite_sha256,
        "case_id": case["id"],
        "numeric_id": case["numeric_id"],
        "adf_file": artifact["adf_file"],
        "adf_sha256": artifact["sha256"]["adf"],
        "payload_file": artifact["payload_file"],
        "payload_sha256": artifact["sha256"]["payload"],
    }
    require_equal(manifest["suite"], expected_suite, f"{manifest_path}: suite")

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
        "suite_manifest": suite_sha256,
        "configuration": sha256_file(run_dir / "config.uae"),
    }
    for input_name, expected_path in expected_files.items():
        input_record = inputs[input_name]
        require_equal(
            Path(input_record["file"]).resolve(),
            expected_path.resolve(),
            f"{manifest_path}: {input_name} path",
        )
        require_equal(
            sha256_file(expected_path),
            expected_hashes[input_name],
            f"{expected_path}: SHA-256",
        )
        require_equal(
            input_record["sha256"],
            expected_hashes[input_name],
            f"{manifest_path}: {input_name} SHA-256",
        )
        if input_name in {"adf", "payload"}:
            require_equal(
                input_record["mode"],
                "0o444",
                f"{manifest_path}: {input_name} mode",
            )
            require_equal(
                stat.S_IMODE(expected_path.stat().st_mode),
                0o444,
                f"{expected_path}: filesystem mode",
            )

    require_equal(
        sha256_file(suite_dir / artifact["adf_file"]),
        sha256_file(expected_files["adf"]),
        f"{run_dir}: ADF differs from suite",
    )
    require_equal(
        sha256_file(suite_dir / artifact["payload_file"]),
        sha256_file(expected_files["payload"]),
        f"{run_dir}: payload differs from suite",
    )
    require_equal(
        sha256_file(suite_dir / "suite-v1.json"),
        sha256_file(expected_files["suite_manifest"]),
        f"{run_dir}: suite manifest differs from source",
    )

    firmware = inputs["firmware"]
    require_equal(
        firmware["sha256"],
        PROFILES[profile]["firmware_sha256"],
        f"{manifest_path}: firmware identity",
    )
    require_equal(
        sha256_file(Path(firmware["file"])),
        firmware["sha256"],
        f"{firmware['file']}: firmware SHA-256",
    )

    require_equal(
        inputs["configuration"]["sha256"],
        sha256_file(run_dir / "config.uae"),
        f"{manifest_path}: configuration SHA-256",
    )
    validate_config(run_dir / "config.uae", profile, case["id"], manifest)
    validate_input_manifests(run_dir, manifest, artifact)

    run_log_path = run_dir / "run.stdout"
    capture_hash_path = run_dir / "capture.sha256"
    require_equal(
        sha256_file(run_log_path),
        manifest["files"]["run_log_sha256"],
        f"{run_log_path}: SHA-256",
    )
    require_equal(
        sha256_file(capture_hash_path),
        manifest["files"]["capture_hash_manifest_sha256"],
        f"{capture_hash_path}: SHA-256",
    )
    return manifest


def load_run(
    capture_root: Path,
    suite_dir: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    suite_sha256: str,
) -> tuple[list[Image.Image], dict[str, Any]]:
    run_dir = capture_root / profile / case["id"]
    manifest = validate_capture_manifest(
        run_dir,
        profile,
        case,
        artifact,
        suite_dir,
        suite_sha256,
    )

    readiness = manifest["readiness"]
    core_fields = readiness["captured_core_fields"]
    guest_fields = readiness["captured_guest_field_counters"]
    require_equal(len(core_fields), FRAME_COUNT, f"{run_dir}: core field count")
    require_equal(
        core_fields,
        list(range(core_fields[0], core_fields[0] + FRAME_COUNT)),
        f"{run_dir}: adjacent core fields",
    )
    require_equal(guest_fields, GUEST_FIELDS, f"{run_dir}: guest fields")
    require_equal(
        readiness["ready_guest_field_counter"],
        1,
        f"{run_dir}: ready guest field",
    )
    require_equal(
        readiness["settle_fields"],
        SETTLE_FIELDS,
        f"{run_dir}: settle fields",
    )
    require_equal(
        readiness["ready_core_field"] + SETTLE_FIELDS,
        core_fields[0],
        f"{run_dir}: ready-to-capture core-field interval",
    )
    require_equal(
        readiness["ready_guest_field_counter"] + SETTLE_FIELDS,
        guest_fields[0],
        f"{run_dir}: ready-to-capture guest-field interval",
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
        f"{run_dir}: raw capture geometry",
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
        require_equal(
            field_record["core_field"], core_field, f"{run_dir}: field core ID"
        )
        require_equal(
            field_record["guest_field_counter"],
            guest_field,
            f"{run_dir}: field guest ID",
        )
        raw_path = run_dir / "capture" / field_record["raw_file"]
        metadata_path = run_dir / "capture" / field_record["metadata_file"]
        require_equal(
            raw_path.name,
            f"field-{core_field:06d}.bgra",
            f"{run_dir}: raw field name",
        )
        require_equal(
            metadata_path.name,
            f"field-{core_field:06d}.json",
            f"{run_dir}: metadata field name",
        )
        raw = raw_path.read_bytes()
        require_equal(
            len(raw), WIDTH * HEIGHT * 4, f"{raw_path}: packed byte count"
        )
        require_equal(
            sha256_bytes(raw),
            field_record["raw_sha256"],
            f"{raw_path}: SHA-256",
        )
        metadata = read_json(metadata_path)
        require_equal(
            sha256_file(metadata_path),
            field_record["metadata_sha256"],
            f"{metadata_path}: SHA-256",
        )
        require_equal(
            metadata,
            field_record["metadata"],
            f"{metadata_path}: embedded metadata",
        )
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
            f"{metadata_path}: metadata",
        )
        expected_capture_files[raw_path.name] = field_record["raw_sha256"]
        expected_capture_files[metadata_path.name] = field_record["metadata_sha256"]
        raw_frames.append(raw)

    require_equal(
        capture_hashes,
        expected_capture_files,
        f"{run_dir / 'capture.sha256'}: captured files",
    )
    if len(set(raw_frames)) != 1:
        raise ValueError(f"{run_dir}: adjacent raw fields are not byte-identical")

    allowed_pixels = {b"\x00\x00\x00\x00", guard_bgra(case)}
    for raw in raw_frames:
        observed_pixels = {raw[index : index + 4] for index in range(0, len(raw), 4)}
        require_equal(
            observed_pixels,
            allowed_pixels,
            f"{run_dir}: raw guard/black pixel set",
        )

    frames = [
        Image.frombytes("RGBA", (WIDTH, HEIGHT), raw, "raw", "BGRA")
        for raw in raw_frames
    ]
    observed_runs = [black_runs(frame) for frame in frames]
    expected_runs = EXPECTED_BLACK_RUNS[(profile, case["id"])]
    if any(runs != expected_runs for runs in observed_runs):
        raise ValueError(
            f"{run_dir}: black runs {observed_runs} do not match {expected_runs}"
        )

    log_text = (run_dir / "run.stdout").read_text(encoding="utf-8")
    complete_marker = (
        f"CODEX_CAPTURE complete first_core_field={core_fields[0]} "
        f"last_core_field={core_fields[-1]} first_guest_field={guest_fields[0]} "
        f"last_guest_field={guest_fields[-1]}"
    )
    if complete_marker not in log_text:
        raise ValueError(f"{run_dir}: completion marker is absent from run log")
    if "CODEX_CAPTURE error=" in log_text:
        raise ValueError(f"{run_dir}: capture error is present in run log")

    return frames, manifest


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
        stride = WIDTH * 4
        filtered = b"".join(
            b"\0" + rgba[row * stride : (row + 1) * stride] for row in range(HEIGHT)
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
        require_equal(packaged.n_frames, len(frames), f"{path}: APNG frame count")
        decoded = [
            frame.convert("RGBA").tobytes()
            for frame in ImageSequence.Iterator(packaged)
        ]
    original = [frame.tobytes() for frame in frames]
    require_equal(decoded, original, f"{path}: APNG decode round trip")
    return sha256_bytes(b"".join(decoded))


def make_record(
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    manifest: dict[str, Any],
    apng_name: str,
    apng_sha256: str,
    decoded_sha256: str,
    config_name: str,
    config_sha256: str,
    log_name: str,
    log_sha256: str,
    manifest_name: str,
    manifest_sha256: str,
) -> dict[str, Any]:
    profile_data = PROFILES[profile]
    core_fields = manifest["readiness"]["captured_core_fields"]
    guest_fields = manifest["readiness"]["captured_guest_field_counters"]
    start, stop = SEMANTIC_EDGES[(profile, case["id"])]
    active_interval = start is not None
    raw_runs = EXPECTED_BLACK_RUNS[(profile, case["id"])]
    run_text = ", ".join(f"[{left}, {right})" for left, right in raw_runs)
    fine_case = case["id"].startswith("aga-fine-")

    notes = [
        (
            f"Raw sample row {SAMPLE_ROW} contained black runs {run_text} in "
            "all three byte-identical adjacent fields."
        ),
        (
            "The raw x=[0, 2) transparent-black producer boundary is retained "
            "in the APNG and the black-run audit, but is not interpreted as a "
            "programmable-HBLANK edge."
        ),
        (
            f"Ready was observed at core field "
            f"{manifest['readiness']['ready_core_field']} with guest field "
            f"{manifest['readiness']['ready_guest_field_counter']}; after "
            f"{SETTLE_FIELDS} fields, core fields {core_fields} and guest "
            f"counters {guest_fields} were captured."
        ),
        (
            "The source-derived raw mapping is "
            "x=4*(register&0xff)+floor(((register>>8)&7)/2)-184. It came "
            "from the producer source path; no image-alignment search was "
            "performed."
        ),
        (
            "The 756 by 576 UAE core framebuffer was preserved in full. The "
            "recorded 752 by 572 FS-UAE frontend compatibility view was not "
            "applied."
        ),
        (
            f"Captured configuration ../configs/{config_name} has SHA-256 "
            f"{config_sha256}."
        ),
        f"Raw run log ../logs/{log_name} has SHA-256 {log_sha256}.",
        (
            f"Capture-time manifest ../manifests/{manifest_name} has SHA-256 "
            f"{manifest_sha256}."
        ),
    ]
    if fine_case:
        notes.append(
            "The AGA fine stop word 0x07a0 produced raw stop sample 459 in "
            f"{case['resolution']} mode."
        )

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
                "tools/fs-uae-hblank-capture/capture.sh "
                f"{profile} {case['id']} "
                "<FS-UAE-5.0.7-f362278ccd4c60991caac3b4d240d4a3f751bea2> "
                "<suite-1.0.1-directory> <firmware-matching-recorded-sha256> "
                "<fresh-output-directory> <operator>"
            ),
            "configuration_sha256": config_sha256,
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
            "file_sha256": apng_sha256,
            "decoded_pixel_sha256": decoded_sha256,
        },
        "normalization": {
            "beam_coordinate": {
                "sample_beam_line": SAMPLE_BEAM_LINE,
                "sample_row": SAMPLE_ROW,
                "horizontal_origin_sample": -184,
                "horizontal_samples_per_register_increment_numerator": 4,
                "horizontal_samples_per_register_increment_denominator": 1,
                "phase_numerator": 0,
                "phase_denominator": 1,
            },
            "crop": {"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT},
            "field_handling": "bob",
            "color_conversion": (
                "lossless BGRA-to-RGBA channel reorder; PNG RGBA8 decoded "
                "without colour management"
            ),
            "alignment_search": False,
        },
        "observations": {
            "guard_color_word": case["line_geometry"]["guard_color_word"],
            "blank_start_samples": [start] * FRAME_COUNT,
            "blank_stop_samples": [stop] * FRAME_COUNT,
            "interval_convention": (
                "start-inclusive-stop-exclusive"
                if active_interval
                else "not-applicable"
            ),
            "wrap_outcome": (
                "wraps"
                if case["id"] == "programmed-wrap"
                else "does-not-wrap"
                if active_interval
                else "not-applicable"
            ),
            "equal_outcome": (
                "empty" if case["id"] == "programmed-equal" else "not-applicable"
            ),
            "uncertainty_samples": 0,
            "notes": notes,
        },
        "provenance": {
            "operator": manifest["capture"]["operator"],
            "capture_date": manifest["capture"]["captured_at_utc"][:10],
            "host": manifest["capture"]["host"],
            "classification": "software-derived",
        },
    }


def package(capture_root: Path, suite_dir: Path, output_root: Path) -> None:
    suite, suite_sha256 = validate_suite(suite_dir)
    validate_topology(capture_root)
    producer_build_path = output_root / "producer-build-v1.json"
    require_equal(
        sha256_file(producer_build_path),
        PRODUCER_BUILD_SHA256,
        f"{producer_build_path}: SHA-256",
    )
    case_by_id = {case["id"]: case for case in suite["cases"]}
    artifact_by_id = {
        artifact["case_id"]: artifact for artifact in suite["artifacts"]
    }

    captures_dir = output_root / "captures"
    records_dir = output_root / "records"
    logs_dir = output_root / "logs"
    configs_dir = output_root / "configs"
    manifests_dir = output_root / "manifests"
    for directory in (
        captures_dir,
        records_dir,
        logs_dir,
        configs_dir,
        manifests_dir,
    ):
        directory.mkdir(parents=True, exist_ok=True)

    packaged_runs: list[dict[str, Any]] = []
    configurations: dict[str, str] = {}
    for profile, profile_data in PROFILES.items():
        for case_id in profile_data["case_ids"]:
            case = case_by_id[case_id]
            artifact = artifact_by_id[case_id]
            frames, manifest = load_run(
                capture_root,
                suite_dir,
                profile,
                case,
                artifact,
                suite_sha256,
            )
            run_dir = capture_root / profile / case_id
            stem = f"{profile}--{case_id}"

            config_name = f"{stem}.uae"
            log_name = f"{stem}.log"
            manifest_name = f"{stem}.json"
            config_path = configs_dir / config_name
            log_path = logs_dir / log_name
            manifest_path = manifests_dir / manifest_name
            shutil.copyfile(run_dir / "config.uae", config_path)
            shutil.copyfile(run_dir / "run.stdout", log_path)
            shutil.copyfile(run_dir / "capture-manifest.json", manifest_path)
            config_sha256 = sha256_file(config_path)
            log_sha256 = sha256_file(log_path)
            manifest_sha256 = sha256_file(manifest_path)
            configurations[stem] = config_sha256

            apng_path = captures_dir / f"{stem}.apng"
            decoded_sha256 = write_apng(apng_path, frames)
            apng_sha256 = sha256_file(apng_path)
            record = make_record(
                profile,
                case,
                artifact,
                manifest,
                apng_path.name,
                apng_sha256,
                decoded_sha256,
                config_name,
                config_sha256,
                log_name,
                log_sha256,
                manifest_name,
                manifest_sha256,
            )
            record_path = records_dir / f"{stem}.json"
            record_path.write_text(
                json.dumps(record, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            packaged_runs.append(
                {
                    "profile": profile,
                    "case_id": case_id,
                    "configuration_file": f"configs/{config_name}",
                    "configuration_sha256": config_sha256,
                    "capture_manifest_file": f"manifests/{manifest_name}",
                    "capture_manifest_sha256": manifest_sha256,
                    "run_log_file": f"logs/{log_name}",
                    "run_log_sha256": log_sha256,
                    "capture_file": f"captures/{apng_path.name}",
                    "capture_sha256": apng_sha256,
                    "decoded_pixel_sha256": decoded_sha256,
                    "record_file": f"records/{record_path.name}",
                    "record_sha256": sha256_file(record_path),
                }
            )
            print(stem)

    expected_stems = {
        f"{profile}--{case_id}"
        for profile, profile_data in PROFILES.items()
        for case_id in profile_data["case_ids"]
    }
    directory_contracts = [
        (captures_dir, ".apng"),
        (records_dir, ".json"),
        (logs_dir, ".log"),
        (configs_dir, ".uae"),
        (manifests_dir, ".json"),
    ]
    for directory, suffix in directory_contracts:
        actual_stems = {path.stem for path in directory.glob(f"*{suffix}")}
        require_equal(
            actual_stems,
            expected_stems,
            f"{directory}: stale or missing packaged files",
        )

    package_manifest = {
        "schema_version": "1.0.0",
        "suite": {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": SUITE_SOURCE_REVISION,
            "manifest_sha256": suite_sha256,
        },
        "producer": {
            "product": PRODUCER["product"],
            "version": PRODUCER["version"],
            "revision": PRODUCER["revision"],
            "source_url": PRODUCER["source_url"],
            "uae_base_version": PRODUCER["uae_base_version"],
            "binary_sha256": PRODUCER["binary_sha256"],
            "capture_patch_sha256": PRODUCER["capture_patch_sha256"],
            "build_manifest_file": "producer-build-v1.json",
            "build_manifest_sha256": PRODUCER_BUILD_SHA256,
        },
        "capture_tools": CAPTURE_TOOLS,
        "matrix": {
            "profiles": list(PROFILES),
            "run_count": len(expected_stems),
            "raw_width": WIDTH,
            "raw_height": HEIGHT,
            "raw_pixel_format": "BGRA8888",
            "packaged_pixel_format": "RGBA8888",
            "sample_row": SAMPLE_ROW,
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
        help="reference-package directory (default: directory containing this script)",
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
