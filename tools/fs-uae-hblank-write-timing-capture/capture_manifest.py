#!/usr/bin/env python3
"""Identify and validate one FS-UAE HBLANK write-timing raw capture."""

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


SUITE_ID = "org.198x.amiga.programmable-hblank-write-timing"
SUITE_VERSION = "1.0.0"
SOURCE_REVISION = "f362278ccd4c60991caac3b4d240d4a3f751bea2"
BINARY_SHA256 = "81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b"
PATCH_SHA256 = "73e423453152097723b22e4ba0db7cb626b4756e5697d49154bfd98055ddd0ed"
EXPECTED_ROM_SHA256 = {
    "ecs": "d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79",
    "aga": "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707",
}
READY_RE = re.compile(
    r"^CODEX_READY core_field=(\d+) guest_field=(\d+) case=(\d+) "
    r"schema=(\d+) magic=HBLK$",
    re.MULTILINE,
)
COMPLETE_RE = re.compile(
    r"^CODEX_CAPTURE complete first_core_field=(\d+) last_core_field=(\d+) "
    r"first_guest_field=(\d+) last_guest_field=(\d+)$",
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
    return {
        name: sha256_file(root / name)
        for name in ("capture.sh", "capture_manifest.py", "config.uae.in")
    }


def load_suite(path: Path) -> dict[str, Any]:
    suite = json.loads(path.read_text(encoding="utf-8"))
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
        artifact for artifact in suite["artifacts"]
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
        f"'config_description' <- '198x HBLANK write timing {profile} {case_id}'",
        f"'floppy0' <- '{run_dir}/inputs/{case_id}.adf'",
        "'gfx_resolution' <- 'hires'",
        "'gfx_linemode' <- 'double2'",
        "'gfx_overscanmode' <- 'overscan'",
        "'floppy_write_protect' <- 'true'",
    ]
    if profile == "ecs":
        expected_markers += [
            "'chipset' <- 'ecs'",
            "'chipset_compatible' <- 'A500+'",
            "CPU=68000, FPU=0, MMU=0, JIT=0. prefetch and cycle-exact 24-bit",
            "Known ROM 'KS ROM v2.04 (A500+)' loaded",
        ]
    else:
        expected_markers += [
            "'chipset' <- 'aga'",
            "'chipset_compatible' <- 'A1200'",
            "'cpu_type' <- '68ec020'",
            "CPU=68020, FPU=0, MMU=0, JIT=0. ~cycle-exact 24-bit",
            "Known ROM 'KS ROM v3.1 (A1200)' loaded",
        ]
    missing = [marker for marker in expected_markers if marker not in log_text]
    if missing:
        raise ValueError(f"run log lacks effective-configuration markers: {missing}")
    if re.search(r"^CODEX_CAPTURE (?:error|discontinuity)", log_text, re.MULTILINE):
        raise ValueError("capture hook reported an error or discontinuity")
    return ready, complete


def write_manifest(args: argparse.Namespace) -> None:
    run_dir = args.run_dir.resolve()
    profile = args.profile
    case_id = args.case_id
    binary = args.binary.resolve()
    firmware = args.firmware.resolve()
    if profile not in EXPECTED_ROM_SHA256:
        raise ValueError("profile must be ecs or aga")
    if sha256_file(binary) != BINARY_SHA256:
        raise ValueError("producer binary hash mismatch")
    if sha256_file(firmware) != EXPECTED_ROM_SHA256[profile]:
        raise ValueError("firmware hash mismatch")
    before = (run_dir / "inputs-before.sha256").read_bytes()
    after = (run_dir / "inputs-after.sha256").read_bytes()
    if before != after:
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

    log_path = run_dir / "run.stdout"
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    ready, complete = validate_log(
        log_text, run_dir, profile, case_id, numeric_id
    )

    raw_paths = sorted((run_dir / "capture").glob("field-*.bgra"))
    metadata_paths = sorted((run_dir / "capture").glob("field-*.json"))
    if len(raw_paths) != 3 or len(metadata_paths) != 3:
        raise ValueError("expected exactly three raw fields and metadata files")

    fields: list[dict[str, Any]] = []
    raw_hashes: list[str] = []
    core_fields: list[int] = []
    guest_fields: list[int] = []
    for raw_path, metadata_path in zip(raw_paths, metadata_paths, strict=True):
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        core_field = metadata["core_field"]
        guest_field = metadata["guest_field_counter"]
        if raw_path.stem != f"field-{core_field:06d}":
            raise ValueError(f"{raw_path}: field name and metadata disagree")
        if metadata_path.stem != raw_path.stem:
            raise ValueError("raw and metadata field names disagree")
        if metadata["ready"]["case_number"] != numeric_id:
            raise ValueError("field metadata case number mismatch")
        if metadata["ready"]["schema_version"] != 1:
            raise ValueError("field metadata ready schema mismatch")
        framebuffer = metadata["framebuffer"]
        if framebuffer != {
            "stage": (
                "completed UAE chipset video_memory before FS-UAE "
                "compatibility crop and frontend processing"
            ),
            "width": 756,
            "height": 576,
            "stride_bytes": 8192,
            "depth_bits": 32,
            "pixel_format": "BGRA8888",
            "packed_output_stride_bytes": 3024,
            "complete": True,
        }:
            raise ValueError("unexpected raw framebuffer metadata")
        if raw_path.stat().st_size != 756 * 576 * 4:
            raise ValueError(f"{raw_path}: unexpected raw size")
        raw_sha256 = sha256_file(raw_path)
        raw_hashes.append(raw_sha256)
        core_fields.append(core_field)
        guest_fields.append(guest_field)
        fields.append(
            {
                "core_field": core_field,
                "guest_field_counter": guest_field,
                "raw_file": raw_path.name,
                "raw_sha256": raw_sha256,
                "metadata_file": metadata_path.name,
                "metadata_sha256": sha256_file(metadata_path),
                "metadata": metadata,
            }
        )

    if len(set(raw_hashes)) != 1:
        raise ValueError("adjacent raw fields are not byte-identical")
    if core_fields != list(range(core_fields[0], core_fields[0] + 3)):
        raise ValueError("core field labels are not adjacent")
    if guest_fields != list(range(guest_fields[0], guest_fields[0] + 3)):
        raise ValueError("guest field counters are not adjacent")
    if guest_fields[0] != 9:
        raise ValueError("first captured guest field counter is not nine")

    ready_core = int(ready.group(1))
    ready_guest = int(ready.group(2))
    complete_core = [int(complete.group(1)), int(complete.group(2))]
    complete_guest = [int(complete.group(3)), int(complete.group(4))]
    if complete_core != [core_fields[0], core_fields[-1]]:
        raise ValueError("completion core labels disagree with field metadata")
    if complete_guest != [guest_fields[0], guest_fields[-1]]:
        raise ValueError("completion guest counters disagree with field metadata")
    if core_fields[0] - ready_core != 8 or guest_fields[0] - ready_guest != 8:
        raise ValueError("capture did not settle eight observed fields")

    script_root = Path(__file__).resolve().parent
    manifest = {
        "schema_version": "1.0.0",
        "capture": {
            "profile": profile,
            "case_id": case_id,
            "captured_at_utc": args.captured_at_utc,
            "operator": args.operator,
            "host": args.host,
            "command": [
                str(binary),
                str(run_dir / "config.uae"),
            ],
            "environment": {
                "FSEMU_QUIT_AFTER_N_FRAMES": "600",
                "FSEMU_CODEX_CAPTURE_DIR": str(run_dir / "capture"),
                "FSEMU_CODEX_CAPTURE_CASE_NUMBER": str(numeric_id),
                "FSEMU_CODEX_CAPTURE_MIN_FIELD_COUNTER": "9",
            },
        },
        "suite": {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": suite["suite"]["source_revision"],
            "case_id": case_id,
            "numeric_id": numeric_id,
            "manifest_sha256": sha256_file(suite_path),
            "adf_file": artifact["adf_file"],
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": artifact["payload_file"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "product": "FS-UAE",
            "version": "5.0.7",
            "revision": SOURCE_REVISION,
            "uae_base_version": "WinUAE 6.0.1",
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
            "configuration": {
                "file": str(run_dir / "config.uae"),
                "sha256": sha256_file(run_dir / "config.uae"),
            },
            "suite_manifest": {
                "file": str(suite_path),
                "sha256": sha256_file(suite_path),
            },
            "adf": {
                "file": str(adf_path),
                "sha256": sha256_file(adf_path),
                "mode": oct(adf_path.stat().st_mode & 0o777),
            },
            "payload": {
                "file": str(payload_path),
                "sha256": sha256_file(payload_path),
                "mode": oct(payload_path.stat().st_mode & 0o777),
            },
            "before_manifest_sha256": sha256_file(
                run_dir / "inputs-before.sha256"
            ),
            "after_manifest_sha256": sha256_file(
                run_dir / "inputs-after.sha256"
            ),
            "unchanged_during_capture": True,
        },
        "capture_tools": {
            **tool_hashes(),
            "directory": str(script_root),
        },
        "readiness": {
            "ready_core_field": ready_core,
            "ready_guest_field_counter": ready_guest,
            "settle_fields": core_fields[0] - ready_core,
            "captured_core_fields": core_fields,
            "captured_guest_field_counters": guest_fields,
        },
        "raw_capture": {
            "width": 756,
            "height": 576,
            "pixel_format": "BGRA8888",
            "packed_stride_bytes": 3024,
            "producer_stride_bytes": 8192,
            "fields": fields,
            "adjacent_field_stability": "byte-identical",
        },
        "files": {
            "run_log_sha256": sha256_file(log_path),
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
            write_manifest(args)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
