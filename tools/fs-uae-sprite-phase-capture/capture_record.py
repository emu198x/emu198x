#!/usr/bin/env python3
"""Validate, measure, and describe one FS-UAE sprite-phase capture."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable


SUITE_ID = "org.198x.amiga.sprite-horizontal-phase"
SUITE_VERSION = "1.0.0"
SUITE_SHA256 = "1390ffb208e1829f2fe1c12f1aae90e7a1b1981cdcf8cb2426b1da8611b4301b"
SOURCE_REVISION = "f362278ccd4c60991caac3b4d240d4a3f751bea2"
BINARY_SHA256 = "87b7efbb9c50c1f6d6b7fee22d165a109e0abab791f99add96df0c870b242e96"
PATCH_SHA256 = "f423049a6d93fe6534aa9d6fc99a355147e7a509b3a46be68b1ed85516d043a0"

WIDTH = 756
HEIGHT = 576
PIXEL_BYTES = 4
PACKED_STRIDE = WIDTH * PIXEL_BYTES
PRODUCER_STRIDE = 8192
SAMPLE_BEAM_LINE = 132
SAMPLE_ROW = 210
FIELD_COUNT = 3
FIRST_CAPTURED_GUEST_FIELD = 9
SETTLE_FIELDS = 8
TRANSPARENT_BLACK = bytes((0, 0, 0, 0))

EXPECTED_ROM_SHA256 = {
    "ocs": "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53",
    "aga": "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707",
}

PROFILES: dict[str, dict[str, Any]] = {
    "ocs": {
        "model": "Amiga 500",
        "cpu": "Motorola 68000",
        "agnus_or_alice": "OCS Agnus",
        "denise_or_lisa": "OCS Denise",
        "chipset": "OCS",
        "chip_ram_bytes": 524_288,
        "firmware_revision": "Kickstart 1.3 revision 34.005",
        "marker_bgra": bytes((0, 255, 0, 255)),
        "sprite_bgra": bytes((0, 0, 255, 255)),
        "config": {
            "chipset": "ocs",
            "chipset_compatible": "A500",
            "chipmem_size": "1",
            "cpu_type": "68000",
            "cpu_model": "68000",
        },
    },
    "aga": {
        "model": "Amiga 1200",
        "cpu": "Motorola 68EC020",
        "agnus_or_alice": "AGA Alice",
        "denise_or_lisa": "AGA Lisa",
        "chipset": "AGA",
        "chip_ram_bytes": 2_097_152,
        "firmware_revision": "Kickstart 3.1 revision 40.068",
        "marker_bgra": bytes((0, 240, 0, 255)),
        "sprite_bgra": bytes((0, 0, 240, 255)),
        "config": {
            "chipset": "aga",
            "chipset_compatible": "A1200",
            "chipmem_size": "4",
            "cpu_type": "68ec020",
            "cpu_model": "68020",
        },
    },
}

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

READY_RE = re.compile(
    r"^CODEX_SPRITE_READY core_field=(\d+) guest_field=(\d+) case=(\d+) "
    r"schema=(\d+) magic=SPHX$",
    re.MULTILINE,
)
COMPLETE_RE = re.compile(
    r"^CODEX_SPRITE_CAPTURE complete first_core_field=(\d+) "
    r"last_core_field=(\d+) first_guest_field=(\d+) last_guest_field=(\d+)$",
    re.MULTILINE,
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def load_suite(path: Path) -> dict[str, Any]:
    if sha256_file(path) != SUITE_SHA256:
        raise ValueError(f"{path}: suite manifest SHA-256 mismatch")
    suite = load_json(path)
    if suite["suite"]["id"] != SUITE_ID:
        raise ValueError(f"{path}: unexpected suite identifier")
    if suite["suite"]["version"] != SUITE_VERSION:
        raise ValueError(f"{path}: unexpected suite version")
    return suite


def find_case(suite: dict[str, Any], case_id: str) -> dict[str, Any]:
    matches = [case for case in suite["cases"] if case["id"] == case_id]
    if len(matches) != 1:
        raise ValueError(f"case is absent or duplicated: {case_id}")
    return matches[0]


def find_artifact(suite: dict[str, Any], case_id: str) -> dict[str, Any]:
    matches = [
        artifact
        for artifact in suite["artifacts"]
        if artifact["case_id"] == case_id
    ]
    if len(matches) != 1:
        raise ValueError(f"artifact is absent or duplicated: {case_id}")
    return matches[0]


def host_identity() -> str:
    product = subprocess.check_output(
        ["sw_vers", "-productVersion"], text=True
    ).strip()
    build = subprocess.check_output(
        ["sw_vers", "-buildVersion"], text=True
    ).strip()
    return f"macOS {product} build {build}, {platform.machine()}"


def parse_configuration(path: Path) -> dict[str, str]:
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


def validate_configuration(
    path: Path,
    run_dir: Path,
    profile: str,
    case_id: str,
    firmware: Path,
) -> None:
    values = parse_configuration(path)
    expected = {
        "config_description": f"198x sprite horizontal phase {profile} {case_id}",
        "config_version": "6.0.1",
        "floppy_write_protect": "true",
        "floppy0wp": "true",
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
        **PROFILES[profile]["config"],
    }
    for key, expected_value in expected.items():
        if values.get(key) != expected_value:
            raise ValueError(
                f"{path}: {key} is {values.get(key)!r}, expected {expected_value!r}"
            )
    if Path(values["kickstart_rom_file"]).resolve() != firmware.resolve():
        raise ValueError(f"{path}: Kickstart path differs from the captured input")
    expected_adf = (run_dir / f"inputs/{case_id}.adf").resolve()
    if Path(values["floppy0"]).resolve() != expected_adf:
        raise ValueError(f"{path}: floppy path differs from the staged ADF")


def expected_ready(case: dict[str, Any]) -> dict[str, Any]:
    registers = case["registers"]
    register_names = (
        "spr0pos",
        "spr0ctl",
        "spr0data",
        "spr0datb",
    )
    ready: dict[str, Any] = {
        "magic": "SPHX",
        "case_number": case["numeric_id"],
        "schema_version": 1,
        "identity": case["identity"]["serial"],
        "sample_beam_line": case["geometry"]["sample_beam_line"],
    }
    for name in register_names:
        ready[name] = f"0x{int(registers[name], 16):04x}"
    return ready


def validate_ready(
    metadata_ready: dict[str, Any],
    case: dict[str, Any],
) -> None:
    expected = expected_ready(case)
    for key, expected_value in expected.items():
        if metadata_ready.get(key) != expected_value:
            raise ValueError(
                f"ready record {key} is {metadata_ready.get(key)!r}, "
                f"expected {expected_value!r}"
            )


def validate_log(
    log_text: str,
    run_dir: Path,
    profile: str,
    case_id: str,
    numeric_id: int,
) -> tuple[re.Match[str], re.Match[str]]:
    ready_matches = list(READY_RE.finditer(log_text))
    complete_matches = list(COMPLETE_RE.finditer(log_text))
    if len(ready_matches) != 1 or len(complete_matches) != 1:
        raise ValueError("run log does not contain one ready and completion marker")
    ready = ready_matches[0]
    complete = complete_matches[0]
    if int(ready.group(3)) != numeric_id or int(ready.group(4)) != 1:
        raise ValueError("run log ready identity mismatch")

    expected_markers = [
        f"'config_description' <- '198x sprite horizontal phase {profile} {case_id}'",
        f"'floppy0' <- '{run_dir}/inputs/{case_id}.adf'",
        "'gfx_resolution' <- 'hires'",
        "'gfx_linemode' <- 'double2'",
        "'gfx_overscanmode' <- 'overscan'",
        "'floppy_write_protect' <- 'true'",
        f"'chipset' <- '{PROFILES[profile]['config']['chipset']}'",
        f"'chipset_compatible' <- '{PROFILES[profile]['config']['chipset_compatible']}'",
        "CPU=68000, FPU=0, MMU=0, JIT=0. prefetch and cycle-exact 24-bit"
        if profile == "ocs"
        else "CPU=68020, FPU=0, MMU=0, JIT=0. ~cycle-exact 24-bit",
    ]
    missing = [marker for marker in expected_markers if marker not in log_text]
    if missing:
        raise ValueError(f"run log lacks effective-configuration markers: {missing}")
    if re.search(
        r"^CODEX_SPRITE_CAPTURE (?:error|discontinuity)", log_text, re.MULTILINE
    ):
        raise ValueError("capture hook reported an error or discontinuity")
    return ready, complete


def pixel_at(row: bytes, sample: int) -> bytes:
    start = sample * PIXEL_BYTES
    return row[start : start + PIXEL_BYTES]


def matching_runs(
    row: bytes,
    width: int,
    predicate: Callable[[bytes], bool],
) -> list[tuple[int, int]]:
    runs: list[tuple[int, int]] = []
    start: int | None = None
    for sample in range(width + 1):
        matches = sample < width and predicate(pixel_at(row, sample))
        if matches and start is None:
            start = sample
        elif not matches and start is not None:
            runs.append((start, sample))
            start = None
    return runs


def require_one_run(
    row: bytes,
    width: int,
    expected_pixel: bytes,
    label: str,
) -> tuple[int, int]:
    runs = matching_runs(row, width, lambda pixel: pixel == expected_pixel)
    if len(runs) != 1:
        formatted = [f"[{start},{stop})" for start, stop in runs]
        raise ValueError(
            f"sample row has {len(runs)} {label} intervals, expected one: {formatted}"
        )
    return runs[0]


def measure_field(
    raw: bytes,
    profile: str,
    *,
    width: int = WIDTH,
    height: int = HEIGHT,
    sample_row: int = SAMPLE_ROW,
) -> dict[str, Any]:
    expected_size = width * height * PIXEL_BYTES
    if len(raw) != expected_size:
        raise ValueError(
            f"raw field has {len(raw)} bytes, expected {expected_size}"
        )
    if not 0 <= sample_row < height:
        raise ValueError("sample row lies outside the raw field")
    row_start = sample_row * width * PIXEL_BYTES
    row = raw[row_start : row_start + width * PIXEL_BYTES]

    hblank_stop = 0
    while hblank_stop < width and pixel_at(row, hblank_stop) == TRANSPARENT_BLACK:
        hblank_stop += 1
    if hblank_stop == 0 or hblank_stop == width:
        raise ValueError("sample row lacks one finite leading transparent-black run")

    marker = require_one_run(
        row, width, PROFILES[profile]["marker_bgra"], "marker"
    )
    sprite = require_one_run(
        row, width, PROFILES[profile]["sprite_bgra"], "sprite"
    )
    return {
        "hblank_status": "observed",
        "hblank_stop_sample": hblank_stop,
        "marker": {
            "status": "observed",
            "start_sample": marker[0],
            "stop_sample": marker[1],
        },
        "sprite": {
            "status": "observed",
            "start_sample": sprite[0],
            "stop_sample": sprite[1],
        },
        "sprite_start_minus_hblank_stop_samples": sprite[0] - hblank_stop,
        "sprite_start_minus_marker_start_samples": sprite[0] - marker[0],
        "uncertainty_samples": 0,
    }


def tool_hashes() -> dict[str, str]:
    root = Path(__file__).resolve().parent
    names = (
        "capture.sh",
        "capture_record.py",
        "config.uae.in",
        "fs-uae-5.0.7-sprite-phase-capture.patch",
    )
    return {name: sha256_file(root / name) for name in names}


def write_capture(args: argparse.Namespace) -> None:
    run_dir = args.run_dir.resolve()
    profile = args.profile
    case_id = args.case_id
    binary = args.binary.resolve()
    firmware = args.firmware.resolve()
    if profile not in PROFILES:
        raise ValueError("profile must be ocs or aga")
    if sha256_file(binary) != BINARY_SHA256:
        raise ValueError("producer binary SHA-256 mismatch")
    if sha256_file(firmware) != EXPECTED_ROM_SHA256[profile]:
        raise ValueError("firmware SHA-256 mismatch")
    patch_path = Path(__file__).resolve().parent / (
        "fs-uae-5.0.7-sprite-phase-capture.patch"
    )
    if sha256_file(patch_path) != PATCH_SHA256:
        raise ValueError("capture patch SHA-256 mismatch")
    if (run_dir / "inputs-before.sha256").read_bytes() != (
        run_dir / "inputs-after.sha256"
    ).read_bytes():
        raise ValueError("capture inputs changed during execution")

    suite_path = run_dir / "inputs/suite-v1.json"
    suite = load_suite(suite_path)
    case = find_case(suite, case_id)
    artifact = find_artifact(suite, case_id)
    numeric_id = case["numeric_id"]
    adf_path = run_dir / f"inputs/{case_id}.adf"
    payload_path = run_dir / f"inputs/{case_id}.bin"
    if sha256_file(adf_path) != artifact["sha256"]["adf"]:
        raise ValueError("ADF hash does not match suite manifest")
    if sha256_file(payload_path) != artifact["sha256"]["payload"]:
        raise ValueError("payload hash does not match suite manifest")

    config_path = run_dir / "config.uae"
    validate_configuration(
        config_path, run_dir, profile, case_id, firmware
    )
    log_path = run_dir / "run.stdout"
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    ready_match, complete_match = validate_log(
        log_text, run_dir, profile, case_id, numeric_id
    )

    raw_paths = sorted((run_dir / "capture").glob("field-*.bgra"))
    metadata_paths = sorted((run_dir / "capture").glob("field-*.json"))
    if len(raw_paths) != FIELD_COUNT or len(metadata_paths) != FIELD_COUNT:
        raise ValueError("expected exactly three raw fields and metadata files")

    fields: list[dict[str, Any]] = []
    observations: list[dict[str, Any]] = []
    raw_hashes: list[str] = []
    core_fields: list[int] = []
    guest_fields: list[int] = []
    ready_records: list[dict[str, Any]] = []
    raw_values: list[bytes] = []
    for raw_path, metadata_path in zip(raw_paths, metadata_paths, strict=True):
        metadata = load_json(metadata_path)
        core_field = metadata["core_field"]
        guest_field = metadata["guest_field_counter"]
        if raw_path.stem != f"field-{core_field:06d}":
            raise ValueError(f"{raw_path}: field name and metadata disagree")
        if metadata_path.stem != raw_path.stem:
            raise ValueError("raw and metadata field names disagree")
        if metadata["framebuffer"] != FRAMEBUFFER:
            raise ValueError("unexpected raw framebuffer metadata")
        ready_record = metadata["ready"]
        validate_ready(ready_record, case)
        raw = raw_path.read_bytes()
        measurement = measure_field(raw, profile)
        measurement.update({"field": guest_field, "sample_row": SAMPLE_ROW})

        raw_sha256 = hashlib.sha256(raw).hexdigest()
        raw_hashes.append(raw_sha256)
        core_fields.append(core_field)
        guest_fields.append(guest_field)
        ready_records.append(ready_record)
        raw_values.append(raw)
        observations.append(measurement)
        fields.append(
            {
                "core_field": core_field,
                "guest_field_counter": guest_field,
                "raw_file": raw_path.name,
                "raw_sha256": raw_sha256,
                "metadata_file": metadata_path.name,
                "metadata_sha256": sha256_file(metadata_path),
                "measurement": measurement,
            }
        )

    if len(set(raw_hashes)) != 1:
        raise ValueError("adjacent raw fields are not byte-identical")
    if any(record != ready_records[0] for record in ready_records[1:]):
        raise ValueError("ready records changed across captured fields")
    if core_fields != list(range(core_fields[0], core_fields[0] + FIELD_COUNT)):
        raise ValueError("core field labels are not adjacent")
    if guest_fields != list(range(guest_fields[0], guest_fields[0] + FIELD_COUNT)):
        raise ValueError("guest field counters are not adjacent")
    if guest_fields[0] != FIRST_CAPTURED_GUEST_FIELD:
        raise ValueError("first captured guest field counter is not nine")

    ready_core = int(ready_match.group(1))
    ready_guest = int(ready_match.group(2))
    complete_core = [int(complete_match.group(1)), int(complete_match.group(2))]
    complete_guest = [int(complete_match.group(3)), int(complete_match.group(4))]
    if complete_core != [core_fields[0], core_fields[-1]]:
        raise ValueError("completion core labels disagree with field metadata")
    if complete_guest != [guest_fields[0], guest_fields[-1]]:
        raise ValueError("completion guest counters disagree with field metadata")
    if core_fields[0] - ready_core != SETTLE_FIELDS:
        raise ValueError("capture did not settle eight observed core fields")
    if guest_fields[0] - ready_guest != SETTLE_FIELDS:
        raise ValueError("capture did not settle eight observed guest fields")

    evidence_dir = run_dir / "evidence"
    evidence_dir.mkdir()
    stream_path = evidence_dir / "fields.bgra"
    with stream_path.open("wb") as output:
        for raw in raw_values:
            output.write(raw)
    evidence_config = evidence_dir / "config.uae"
    shutil.copyfile(config_path, evidence_config)
    stream_sha256 = sha256_file(stream_path)

    profile_data = PROFILES[profile]
    capture_record = {
        "schema_version": "1.0.0",
        "suite_id": SUITE_ID,
        "suite_version": SUITE_VERSION,
        "case_id": case_id,
        "artifact": {
            "adf_file": artifact["adf_file"],
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": artifact["payload_file"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "kind": "software-emulator",
            "product": "FS-UAE",
            "version": "5.0.7",
            "revision": SOURCE_REVISION,
            "source_url": "https://github.com/FrodeSolheim/fs-uae",
            "implementation_family": "UAE",
        },
        "machine": {
            "model": profile_data["model"],
            "cpu": profile_data["cpu"],
            "agnus_or_alice": profile_data["agnus_or_alice"],
            "denise_or_lisa": profile_data["denise_or_lisa"],
            "chipset": profile_data["chipset"],
            "region": "PAL",
            "chip_ram_bytes": profile_data["chip_ram_bytes"],
            "firmware": {
                "revision": profile_data["firmware_revision"],
                "sha256": EXPECTED_ROM_SHA256[profile],
            },
        },
        "execution": {
            "cold_boot": True,
            "command_or_procedure": (
                "Cold boot by tools/fs-uae-sprite-phase-capture/capture.sh; "
                "capture-only hook waited for SPHX field counter 9 and copied "
                "three adjacent completed chipset framebuffers."
            ),
            "configuration_file_name": evidence_config.name,
            "configuration_sha256": sha256_file(evidence_config),
            "ready_rule": {
                "record_address": "0x0002ff00",
                "magic": "SPHX",
                "case_number": numeric_id,
                "schema_version": 1,
                "field_counter_minimum": FIRST_CAPTURED_GUEST_FIELD,
                "byte_order": "big-endian",
            },
            "ready_observed_field": ready_guest,
            "settle_fields": SETTLE_FIELDS,
            "captured_fields": guest_fields,
            "adjacent_field_stability": "confirmed",
        },
        "source_capture": {
            "method": (
                "Capture-only FS-UAE hook copied packed rows from the completed "
                "UAE chipset video_memory buffer; the three fields are "
                "concatenated in captured_fields order."
            ),
            "width": WIDTH,
            "height": HEIGHT,
            "pixel_format": (
                "BGRA8888, tightly packed row-major fields concatenated in "
                "execution.captured_fields order"
            ),
            "stride_bytes": PACKED_STRIDE,
            "blanking_retained": True,
            "overscan_retained": True,
            "filter": "none",
            "scaling": "none",
            "shader": "none",
            "automatic_crop": False,
            "file_name": stream_path.name,
            "file_sha256": stream_sha256,
            "decoded_pixel_file_name": stream_path.name,
            "decoded_pixel_sha256": stream_sha256,
        },
        "normalization": {
            "beam_mapping": {
                "sample_beam_line": SAMPLE_BEAM_LINE,
                "sample_rows": [
                    {"field": field, "capture_row": SAMPLE_ROW}
                    for field in guest_fields
                ],
                "horizontal_origin_description": (
                    "Source sample 0 is the retained raw UAE chipset-buffer "
                    "origin; no horizontal shift or crop is applied."
                ),
                "samples_per_lores_pixel_numerator": 2,
                "samples_per_lores_pixel_denominator": 1,
            },
            "measurement_crop": None,
            "field_handling": "separate-fields",
            "color_conversion": "none; measurements use source BGRA bytes",
            "alignment_search": False,
        },
        "observations": {
            "coordinate_unit": "source-capture sample",
            "interval_convention": "start-inclusive-stop-exclusive",
            "measurement_method": (
                "Exact BGRA equality on raw row 210: the finite leading "
                "transparent-black run defines HBLANK stop; the sole green "
                "COLOR01 run defines the marker; the sole red COLOR17 run "
                "defines the sprite."
            ),
            "fields": observations,
            "notes": [
                "The raw and decoded pixel files are identical because the source hook already emits tightly packed BGRA8888.",
                f"Marker BGRA is {profile_data['marker_bgra'].hex()}; sprite BGRA is {profile_data['sprite_bgra'].hex()}.",
            ],
        },
        "provenance": {
            "operator": args.operator,
            "capture_date": args.captured_at_utc[:10],
            "host": args.host,
            "classification": "software-derived",
        },
    }
    capture_path = evidence_dir / "capture.json"
    capture_path.write_text(
        json.dumps(capture_record, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    script_root = Path(__file__).resolve().parent
    manifest = {
        "schema_version": "1.0.0",
        "capture": {
            "profile": profile,
            "case_id": case_id,
            "captured_at_utc": args.captured_at_utc,
            "operator": args.operator,
            "host": args.host,
            "command": [str(binary), str(config_path)],
            "environment": {
                "FSEMU_QUIT_AFTER_N_FRAMES": "600",
                "FSEMU_CODEX_SPRITE_PHASE_CAPTURE_DIR": str(
                    run_dir / "capture"
                ),
                "FSEMU_CODEX_SPRITE_PHASE_CASE_NUMBER": str(numeric_id),
                "FSEMU_CODEX_SPRITE_PHASE_MIN_FIELD_COUNTER": str(
                    FIRST_CAPTURED_GUEST_FIELD
                ),
            },
        },
        "suite": {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": suite["suite"]["source_revision"],
            "manifest_sha256": SUITE_SHA256,
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "product": "FS-UAE",
            "version": "5.0.7",
            "revision": SOURCE_REVISION,
            "uae_base_version": "WinUAE 6.0.1",
            "source_url": "https://github.com/FrodeSolheim/fs-uae",
            "implementation_family": "UAE",
            "binary_file": str(binary),
            "binary_sha256": BINARY_SHA256,
            "capture_patch_sha256": PATCH_SHA256,
        },
        "inputs": {
            "firmware_sha256": EXPECTED_ROM_SHA256[profile],
            "configuration_sha256": sha256_file(config_path),
            "before_manifest_sha256": sha256_file(
                run_dir / "inputs-before.sha256"
            ),
            "after_manifest_sha256": sha256_file(
                run_dir / "inputs-after.sha256"
            ),
            "unchanged_during_capture": True,
        },
        "capture_tools": {**tool_hashes(), "directory": str(script_root)},
        "readiness": {
            "ready_core_field": ready_core,
            "ready_guest_field_counter": ready_guest,
            "settle_fields": SETTLE_FIELDS,
            "captured_core_fields": core_fields,
            "captured_guest_field_counters": guest_fields,
        },
        "raw_capture": {
            "width": WIDTH,
            "height": HEIGHT,
            "pixel_format": "BGRA8888",
            "packed_stride_bytes": PACKED_STRIDE,
            "producer_stride_bytes": PRODUCER_STRIDE,
            "fields": fields,
            "adjacent_field_stability": "byte-identical",
        },
        "evidence": {
            "capture_record": str(capture_path),
            "capture_record_sha256": sha256_file(capture_path),
            "field_stream": str(stream_path),
            "field_stream_sha256": stream_sha256,
        },
        "files": {
            "run_log_sha256": sha256_file(log_path),
            "capture_hash_manifest_sha256": sha256_file(
                run_dir / "capture.sha256"
            ),
        },
    }
    manifest_path = run_dir / "capture-manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    repository_root = script_root.parents[1]
    validator = repository_root / (
        "test-data/commodore/amiga/sprite-horizontal-phase/tools/validate_capture.py"
    )
    subprocess.run(
        [
            sys.executable,
            str(validator),
            "--suite",
            str(suite_path),
            str(capture_path),
        ],
        check=True,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    case_number = subparsers.add_parser("case-number")
    case_number.add_argument("suite_manifest", type=Path)
    case_number.add_argument("case_id")

    subparsers.add_parser("host")

    write = subparsers.add_parser("write")
    write.add_argument("run_dir", type=Path)
    write.add_argument("profile")
    write.add_argument("case_id")
    write.add_argument("binary", type=Path)
    write.add_argument("firmware", type=Path)
    write.add_argument("captured_at_utc")
    write.add_argument("operator")
    write.add_argument("host")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "case-number":
            suite = load_suite(args.suite_manifest)
            print(find_case(suite, args.case_id)["numeric_id"])
        elif args.command == "host":
            print(host_identity())
        else:
            write_capture(args)
    except (
        KeyError,
        OSError,
        ValueError,
        json.JSONDecodeError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
