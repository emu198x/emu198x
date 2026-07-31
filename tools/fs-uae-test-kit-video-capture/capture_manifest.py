#!/usr/bin/env python3
"""Validate and identify one FS-UAE Amiga Test Kit video capture."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SOURCE_REVISION = "f362278ccd4c60991caac3b4d240d4a3f751bea2"
BINARY_SHA256 = "5c3d9e35d100445a5603c5f86a19cc431a7363828053d4ede7d260c2c5d6899f"
PATCH_SHA256 = "6116765eab7036cf756cb3212968675c9d1ca3ef327b8da3e4d194f05ffbb767"
ADF_SHA256 = "abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28"
FIRMWARE_SHA256 = "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"

BOOT_FIELDS = 600
KEY_HOLD_FIELDS = 3
KEY_RELEASE_SETTLE_FIELDS = 1
INTER_KEY_FIELDS = 50

CASE_SPECS: dict[str, dict[str, Any]] = {
    "gradients": {
        "navigation": ["F6", "F1"],
        "settle_fields": 150,
        "behaviour": "static",
    },
    "static-checkerboard": {
        "navigation": ["F6", "F2"],
        "settle_fields": 100,
        "behaviour": "static",
    },
    "alternating-checkerboard": {
        "navigation": ["F6", "F3"],
        "settle_fields": 100,
        "behaviour": "alternating",
    },
    "ebu-bars": {
        "navigation": ["F6", "F4", "F6"],
        "settle_fields": 100,
        "behaviour": "static",
    },
    "dots": {
        "navigation": ["F6", "F5"],
        "settle_fields": 100,
        "behaviour": "static",
    },
    "crosshatch": {
        "navigation": ["F6", "F6"],
        "settle_fields": 100,
        "behaviour": "static",
    },
}

KEY_RE = re.compile(
    r"^CODEX_TESTKIT key core_field=(\d+) key=(F\d+) "
    r"state=(pressed|released)$",
    re.MULTILINE,
)
CONFIGURED_RE = re.compile(
    r"^CODEX_TESTKIT configured case=([^ ]+) boot_fields=(\d+) "
    r"key_hold_fields=(\d+) release_settle_fields=(\d+) "
    r"inter_key_fields=(\d+) settle_fields=(\d+)$",
    re.MULTILINE,
)
CAPTURED_RE = re.compile(
    r"^CODEX_TESTKIT captured=(\d+) case=([^ ]+) core_field=(\d+) "
    r"raw=(.+?) metadata=(.+)$",
    re.MULTILINE,
)
COMPLETE_RE = re.compile(
    r"^CODEX_TESTKIT complete case=([^ ]+) first_core_field=(\d+) "
    r"last_core_field=(\d+)$",
    re.MULTILINE,
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def tool_hashes() -> dict[str, str]:
    root = Path(__file__).resolve().parent
    names = (
        "capture.sh",
        "capture_manifest.py",
        "config.uae.in",
        "Portable.ini",
        "fs-uae-5.0.7-test-kit-video-capture.patch",
    )
    return {name: sha256_file(root / name) for name in names}


def host_identity() -> str:
    product = subprocess.check_output(
        ["sw_vers", "-productVersion"], text=True
    ).strip()
    build = subprocess.check_output(
        ["sw_vers", "-buildVersion"], text=True
    ).strip()
    return f"macOS {product} build {build}, {platform.machine()}"


def find_runtime_portable(binary: Path) -> Path:
    expected = (Path(__file__).resolve().parent / "Portable.ini").read_bytes()
    for parent in binary.resolve().parents:
        candidate = parent / "Portable.ini"
        if candidate.is_file():
            if candidate.read_bytes() != expected:
                raise ValueError(f"runtime Portable.ini differs: {candidate}")
            return candidate
    raise ValueError("no matching Portable.ini found above the FS-UAE binary")


def write_config(args: argparse.Namespace) -> None:
    replacements = {
        "@CASE@": args.case_id,
        "@RUN_DIR@": str(args.run_dir),
        "@ROM@": str(args.firmware),
        "@ADF@": str(args.adf),
    }
    for value in replacements.values():
        if "\n" in value or "\r" in value:
            raise ValueError("configuration values may not contain newlines")
    rendered = args.template.read_text(encoding="utf-8")
    for marker, value in replacements.items():
        if marker not in rendered:
            raise ValueError(f"configuration template lacks {marker}")
        rendered = rendered.replace(marker, value)
    if re.search(r"@[A-Z_]+@", rendered):
        raise ValueError("configuration template retains a marker")
    args.output.write_text(rendered, encoding="utf-8")


def expected_key_events(spec: dict[str, Any]) -> list[tuple[int, str, str]]:
    events: list[tuple[int, str, str]] = []
    field = BOOT_FIELDS
    navigation = spec["navigation"]
    for index, key in enumerate(navigation):
        events.append((field, key, "pressed"))
        field += KEY_HOLD_FIELDS
        events.append((field, key, "released"))
        field += KEY_RELEASE_SETTLE_FIELDS
        if index + 1 < len(navigation):
            field += INTER_KEY_FIELDS
    return events


def expected_first_capture_field(spec: dict[str, Any]) -> int:
    events = expected_key_events(spec)
    final_release = events[-1][0]
    return final_release + KEY_RELEASE_SETTLE_FIELDS + spec["settle_fields"]


def validate_log(
    log_text: str,
    run_dir: Path,
    runtime_portable: Path,
    case_id: str,
    spec: dict[str, Any],
) -> tuple[int, int]:
    expected_markers = [
        f"CODEX_TESTKIT configured case={case_id} boot_fields=600",
        f"'config_description' <- '198x A1200 AGA Test Kit v1.21 {case_id}'",
        f"'floppy0' <- '{run_dir}/inputs/AmigaTestKit.adf'",
        "'chipset' <- 'aga'",
        "'chipset_compatible' <- 'A1200'",
        "'cpu_type' <- '68ec020'",
        "'gfx_resolution' <- 'hires'",
        "'gfx_linemode' <- 'double2'",
        "'gfx_overscanmode' <- 'overscan'",
        "Known ROM 'KS ROM v3.1 (A1200)' loaded",
        "CPU=68020, FPU=0, MMU=0, JIT=0. ~cycle-exact 24-bit",
        "Portable.ini starts with # FS-UAE",
        f"Data dir: {runtime_portable.parent}/Data/",
        f"Cache dir: {runtime_portable.parent}/Cache/",
    ]
    missing = [marker for marker in expected_markers if marker not in log_text]
    if missing:
        raise ValueError(f"run log lacks effective-configuration markers: {missing}")
    if re.search(r"^CODEX_TESTKIT error=", log_text, re.MULTILINE):
        raise ValueError("capture hook reported an error")

    configured = list(CONFIGURED_RE.finditer(log_text))
    expected_configuration = (
        case_id,
        BOOT_FIELDS,
        KEY_HOLD_FIELDS,
        KEY_RELEASE_SETTLE_FIELDS,
        INTER_KEY_FIELDS,
        spec["settle_fields"],
    )
    if len(configured) != 1:
        raise ValueError("run log does not contain exactly one configured marker")
    actual_configuration = (
        configured[0].group(1),
        *(int(configured[0].group(index)) for index in range(2, 7)),
    )
    if actual_configuration != expected_configuration:
        raise ValueError("capture-hook configuration disagrees with the case")

    actual_events = [
        (int(match.group(1)), match.group(2), match.group(3))
        for match in KEY_RE.finditer(log_text)
    ]
    if actual_events != expected_key_events(spec):
        raise ValueError("field-counted keyboard events disagree with the case")

    expected_first = expected_first_capture_field(spec)
    captures = list(CAPTURED_RE.finditer(log_text))
    if len(captures) != 3:
        raise ValueError("run log does not contain exactly three capture markers")
    for index, capture in enumerate(captures, start=1):
        core_field = expected_first + index - 1
        expected_raw = run_dir / f"capture/field-{core_field:06d}.bgra"
        expected_metadata = run_dir / f"capture/field-{core_field:06d}.json"
        actual = (
            int(capture.group(1)),
            capture.group(2),
            int(capture.group(3)),
            Path(capture.group(4)),
            Path(capture.group(5)),
        )
        expected = (index, case_id, core_field, expected_raw, expected_metadata)
        if actual != expected:
            raise ValueError("capture marker disagrees with the fixed schedule")

    completion = list(COMPLETE_RE.finditer(log_text))
    if len(completion) != 1 or completion[0].group(1) != case_id:
        raise ValueError("run log does not contain one matching completion marker")
    return int(completion[0].group(2)), int(completion[0].group(3))


def validate_frontend_view(value: Any) -> None:
    if not isinstance(value, dict):
        raise ValueError("frontend compatibility view is not an object")
    if value.get("width") != 752 or value.get("height") != 572:
        raise ValueError("unexpected FS-UAE compatibility-view geometry")
    if value.get("partial") != 572:
        raise ValueError("FS-UAE compatibility view was not complete")
    limits = value.get("limits")
    if not isinstance(limits, dict) or set(limits) != {"x", "y", "width", "height"}:
        raise ValueError("invalid compatibility-view limits")
    if not all(isinstance(limits[name], int) for name in limits):
        raise ValueError("compatibility-view limits are not integers")


def write_manifest(args: argparse.Namespace) -> None:
    run_dir = args.run_dir.resolve()
    case_id = args.case_id
    spec = CASE_SPECS.get(case_id)
    if spec is None:
        raise ValueError(f"unknown case: {case_id}")

    binary = args.binary.resolve()
    adf = args.adf.resolve()
    firmware = args.firmware.resolve()
    runtime_portable = find_runtime_portable(binary)
    if sha256_file(binary) != BINARY_SHA256:
        raise ValueError("producer binary hash mismatch")
    if sha256_file(adf) != ADF_SHA256:
        raise ValueError("Test Kit ADF hash mismatch")
    if sha256_file(firmware) != FIRMWARE_SHA256:
        raise ValueError("firmware hash mismatch")
    if tool_hashes()["fs-uae-5.0.7-test-kit-video-capture.patch"] != PATCH_SHA256:
        raise ValueError("capture patch hash mismatch")

    before = (run_dir / "inputs-before.sha256").read_bytes()
    after = (run_dir / "inputs-after.sha256").read_bytes()
    if before != after:
        raise ValueError("capture inputs changed during execution")

    log_path = run_dir / "run.stdout"
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    complete_first, complete_last = validate_log(
        log_text, run_dir, runtime_portable, case_id, spec
    )

    raw_paths = sorted((run_dir / "capture").glob("field-*.bgra"))
    metadata_paths = sorted((run_dir / "capture").glob("field-*.json"))
    if len(raw_paths) != 3 or len(metadata_paths) != 3:
        raise ValueError("expected exactly three raw fields and metadata files")

    first_capture_field = expected_first_capture_field(spec)
    fields: list[dict[str, Any]] = []
    raw_hashes: list[str] = []
    core_fields: list[int] = []
    expected_framebuffer = {
        "stage": (
            "completed UAE chipset video_memory before FS-UAE compatibility "
            "crop and frontend processing"
        ),
        "width": 756,
        "height": 576,
        "source_stride_bytes": 8192,
        "packed_stride_bytes": 3024,
        "pixel_format": "BGRA8888",
        "complete": True,
    }
    for index, (raw_path, metadata_path) in enumerate(
        zip(raw_paths, metadata_paths, strict=True)
    ):
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        core_field = metadata.get("core_field")
        if metadata.get("schema") != "org.198x.fs-uae.test-kit-video-raw/v1":
            raise ValueError("unexpected field metadata schema")
        if metadata.get("case_id") != case_id:
            raise ValueError("field metadata case mismatch")
        if metadata.get("capture_index") != index:
            raise ValueError("field metadata capture index mismatch")
        if metadata.get("first_capture_field") != first_capture_field:
            raise ValueError("field metadata first-capture label mismatch")
        if not isinstance(core_field, int):
            raise ValueError("field metadata lacks an integer core field")
        if raw_path.stem != f"field-{core_field:06d}":
            raise ValueError("raw filename and core field disagree")
        if metadata_path.stem != raw_path.stem:
            raise ValueError("raw and metadata filenames disagree")
        if metadata.get("framebuffer") != expected_framebuffer:
            raise ValueError("unexpected raw framebuffer metadata")
        validate_frontend_view(metadata.get("frontend_compatibility_view"))
        if raw_path.stat().st_size != 756 * 576 * 4:
            raise ValueError(f"{raw_path}: unexpected raw size")

        raw_sha256 = sha256_file(raw_path)
        raw_hashes.append(raw_sha256)
        core_fields.append(core_field)
        fields.append(
            {
                "core_field": core_field,
                "raw_file": raw_path.name,
                "raw_sha256": raw_sha256,
                "metadata_file": metadata_path.name,
                "metadata_sha256": sha256_file(metadata_path),
                "metadata": metadata,
            }
        )

    expected_fields = list(range(first_capture_field, first_capture_field + 3))
    if core_fields != expected_fields:
        raise ValueError("captured core fields disagree with the fixed schedule")
    if [complete_first, complete_last] != [core_fields[0], core_fields[-1]]:
        raise ValueError("completion marker disagrees with field metadata")
    if spec["behaviour"] == "static":
        if len(set(raw_hashes)) != 1:
            raise ValueError("static adjacent fields are not byte-identical")
        temporal_relation = "all-byte-identical"
    else:
        if raw_hashes[0] != raw_hashes[2] or raw_hashes[0] == raw_hashes[1]:
            raise ValueError("alternating fields do not have an A-B-A relationship")
        temporal_relation = "first-equals-third-and-differs-from-second"

    frontend_wait_status = int(
        (run_dir / "frontend-wait-status").read_text(encoding="ascii")
    )
    if frontend_wait_status not in (0, 143):
        raise ValueError("unexpected recorded frontend wait status")

    manifest = {
        "schema_version": "1.0.0",
        "capture": {
            "case_id": case_id,
            "captured_at_utc": args.captured_at_utc,
            "operator": args.operator,
            "host": args.host,
            "command": [str(binary), str(run_dir / "config.uae")],
            "environment": {
                "FSEMU_QUIT_AFTER_N_FRAMES": "1000",
                "FSEMU_CODEX_TESTKIT_CAPTURE_DIR": str(run_dir / "capture"),
                "FSEMU_CODEX_TESTKIT_CASE": case_id,
            },
        },
        "suite": {
            "name": "Amiga Test Kit",
            "version": "1.21",
            "source_tag": "testkit-v1.21",
            "source_commit": "9477599d1611da2326f43532dbe563c2848e308b",
            "adf_sha256": ADF_SHA256,
        },
        "machine": {
            "model": "Commodore Amiga A1200",
            "cpu": "68EC020",
            "chipset": "AGA",
            "region": "PAL",
            "chip_ram_bytes": 2 * 1024 * 1024,
            "expansion_ram_bytes": 0,
            "firmware_sha256": FIRMWARE_SHA256,
        },
        "execution": {
            "boot_fields": BOOT_FIELDS,
            "navigation": spec["navigation"],
            "key_hold_fields": KEY_HOLD_FIELDS,
            "key_release_settle_fields": KEY_RELEASE_SETTLE_FIELDS,
            "inter_key_fields": INTER_KEY_FIELDS,
            "final_settle_fields": spec["settle_fields"],
            "first_capture_field": first_capture_field,
            "captured_core_fields": core_fields,
            "behaviour": spec["behaviour"],
        },
        "producer": {
            "product": "FS-UAE",
            "version": "5.0.7",
            "revision": SOURCE_REVISION,
            "uae_base_version": "WinUAE 6.0.1",
            "implementation_family": "UAE",
            "source_url": "https://github.com/FrodeSolheim/fs-uae",
            "binary_file": str(binary),
            "binary_sha256": BINARY_SHA256,
            "capture_patch_sha256": PATCH_SHA256,
        },
        "inputs": {
            "firmware": {
                "file": str(firmware),
                "sha256": sha256_file(firmware),
            },
            "test_kit_adf": {
                "file": str(adf),
                "sha256": sha256_file(adf),
                "mode": oct(adf.stat().st_mode & 0o777),
            },
            "configuration": {
                "file": str(run_dir / "config.uae"),
                "sha256": sha256_file(run_dir / "config.uae"),
            },
            "runtime_portable": {
                "file": str(runtime_portable),
                "sha256": sha256_file(runtime_portable),
            },
            "before_manifest_sha256": sha256_file(
                run_dir / "inputs-before.sha256"
            ),
            "after_manifest_sha256": sha256_file(run_dir / "inputs-after.sha256"),
            "unchanged_during_capture": True,
        },
        "capture_tools": {
            **tool_hashes(),
            "directory": str(Path(__file__).resolve().parent),
        },
        "raw_capture": {
            "stage": "completed UAE chipset framebuffer before frontend processing",
            "width": 756,
            "height": 576,
            "pixel_format": "BGRA8888",
            "packed_stride_bytes": 3024,
            "producer_stride_bytes": 8192,
            "fields": fields,
            "temporal_relation": temporal_relation,
        },
        "files": {
            "run_log_sha256": sha256_file(log_path),
            "frontend_wait_status": frontend_wait_status,
            "capture_hash_manifest_sha256": sha256_file(
                run_dir / "capture.sha256"
            ),
        },
    }
    (run_dir / "capture-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("host")

    resolve = subparsers.add_parser("resolve")
    resolve.add_argument("path", type=Path)

    verify_portable = subparsers.add_parser("verify-portable")
    verify_portable.add_argument("binary", type=Path)

    config = subparsers.add_parser("config")
    config.add_argument("template", type=Path)
    config.add_argument("output", type=Path)
    config.add_argument("case_id")
    config.add_argument("run_dir", type=Path)
    config.add_argument("firmware", type=Path)
    config.add_argument("adf", type=Path)

    write = subparsers.add_parser("write")
    write.add_argument("run_dir", type=Path)
    write.add_argument("case_id")
    write.add_argument("binary", type=Path)
    write.add_argument("adf", type=Path)
    write.add_argument("firmware", type=Path)
    write.add_argument("captured_at_utc")
    write.add_argument("operator")
    write.add_argument("host")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "host":
            print(host_identity())
        elif args.command == "resolve":
            print(args.path.resolve())
        elif args.command == "verify-portable":
            print(find_runtime_portable(args.binary))
        elif args.command == "config":
            write_config(args)
        else:
            write_manifest(args)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
