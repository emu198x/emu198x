#!/usr/bin/env python3
"""Verify raw Copperline runs and emit portable capture records."""

from __future__ import annotations

import argparse
import binascii
import hashlib
import json
import re
import shutil
import struct
import sys
import zlib
from pathlib import Path
from typing import Any

import PIL
from PIL import Image, ImageSequence


PRODUCER_REVISION = "eec5806778dab8b60f3b05fa7ab2428e4e18b073"
PRODUCER_BINARY_SHA256 = (
    "ead4139d547085ad58a9794b17e57e6bf0649e4c6c7040e038f00550030a7fe9"
)
SUITE_ID = "org.198x.amiga.programmable-hblank"
SUITE_VERSION = "1.0.1"
CAPTURE_FIELDS = [400, 401, 402]
WIDTH = 716
HEIGHT = 570
SAMPLE_ROW = 200
EXPECTED_CAPTURE_ENVIRONMENT = {
    "RUST_LOG": "info",
    "COPPERLINE_SHOT_RAW": "1",
    "COPPERLINE_HCENTER": "0",
    "COPPERLINE_OVERSCAN": "full",
    "COPPERLINE_PIXEL_ASPECT": "square",
    "COPPERLINE_DEINTERLACE": "0",
    "COPPERLINE_PHOSPHOR": "0",
    "COPPERLINE_THREADED_RENDER": "0",
    "COPPERLINE_DBG_BREAK": "300aa",
    "COPPERLINE_DBG_DUMP": "2ff00:48",
    "COPPERLINE_DBG_FC": "2ff0a",
    "COPPERLINE_DBG_AFTER": "7.8",
    "COPPERLINE_DBG_UNTIL": "8.2",
    "COPPERLINE_DBG_MAXHITS": "1",
    "COPPERLINE_DUMP_RENDER_META": "1",
}

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
        "configuration_sha256": (
            "a4809260eaf8e00d0e0bffaf638ed448410c12981144f3a8221c19def9a61d69"
        ),
        "probe_fields": [200, 201, 202],
        "log_marker": "chipset=Ecs (agnus=Ecs8375 denise=Ecs8373) video=Pal",
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
        "configuration_sha256": (
            "f36345b7f9ebf1fc63a552c60c19060ab6324d8ec8d7ee3cf45d6120aa1ba9b8"
        ),
        "probe_fields": [281, 282, 283],
        "log_marker": "chipset=Aga (agnus=AgaAlice denise=AgaLisa) video=Pal",
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
]

EXPECTED_RUNS = {
    "fixed-control": [],
    "ecsena-gate": [(316, 444)],
    "extblken-gate": [(316, 444)],
    "blanken-path": [],
    "programmed-central": [(316, 444)],
    "programmed-wrap": [(0, 60), (636, 716)],
    "programmed-equal": [],
}

SEMANTIC_EDGES = {
    "fixed-control": (None, None),
    "ecsena-gate": (316, 444),
    "extblken-gate": (316, 444),
    "blanken-path": (None, None),
    "programmed-central": (316, 444),
    "programmed-wrap": (636, 60),
    "programmed-equal": (None, None),
}

GATE_NOTES = {
    "ecsena-gate": (
        "Copperline produced the programmed interval while ECSENA was clear; "
        "this is a producer disagreement candidate, not a resolved hardware gate."
    ),
    "extblken-gate": (
        "Copperline produced the programmed interval while EXTBLKEN was clear; "
        "this is a producer disagreement candidate, not a resolved hardware gate."
    ),
}

BREAK_RE = re.compile(r"DBG BREAK .* f=(\d+) ")
MEMORY_RE = re.compile(r"mem 0x0002FF00: ((?:[0-9A-F]{4} )+)")
FRAME_RE = re.compile(r"frame-meta idx=(\d+) emu_frame=(\d+) ")
COUNTER_RE = re.compile(r"fc 0x02FF0A=0x[0-9A-F]+ \((\d+)\).* f=(\d+) ")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def capture_tool_hashes() -> dict[str, str]:
    tool_dir = Path(__file__).resolve().parent
    return {
        "capture.sh": sha256_file(tool_dir / "capture.sh"),
        "capture_manifest.py": sha256_file(tool_dir / "capture_manifest.py"),
    }


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


def validate_capture_manifest(
    run_dir: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    suite_sha256: str,
) -> dict[str, Any]:
    manifest_path = run_dir / "capture-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected_producer = {
        "product": "Copperline",
        "version": "0.13.0",
        "revision": PRODUCER_REVISION,
        "source_url": "https://github.com/CopperlineHQ/Copperline",
        "binary_sha256": PRODUCER_BINARY_SHA256,
        "version_output": "copperline 0.13.0",
    }
    if manifest.get("schema_version") != "1.0.0":
        raise ValueError(f"{manifest_path}: unsupported manifest schema")
    if manifest.get("capture_tools") != capture_tool_hashes():
        raise ValueError(f"{manifest_path}: capture tool identity mismatch")
    if manifest.get("producer") != expected_producer:
        raise ValueError(f"{manifest_path}: producer identity mismatch")

    capture = manifest["capture"]
    if capture["profile"] != profile or capture["case_id"] != case["id"]:
        raise ValueError(f"{manifest_path}: capture identity mismatch")
    if capture["environment"] != EXPECTED_CAPTURE_ENVIRONMENT:
        raise ValueError(f"{manifest_path}: capture environment mismatch")
    expected_command = [
        "<verified-copperline-binary>",
        "--config",
        "capture.toml",
        "--noaudio",
        "--dump-frames",
        "frames",
        "--dump-start",
        "8",
        "--dump-count",
        "3",
    ]
    if capture["command"] != expected_command:
        raise ValueError(f"{manifest_path}: capture command mismatch")
    if not capture["operator"].strip() or not capture["host"].strip():
        raise ValueError(f"{manifest_path}: operator or host is empty")
    if not re.fullmatch(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\+00:00",
        capture["captured_at_utc"],
    ):
        raise ValueError(f"{manifest_path}: invalid UTC capture timestamp")

    expected_suite = {
        "id": SUITE_ID,
        "version": SUITE_VERSION,
        "source_revision": "source-v2",
        "case_id": case["id"],
        "numeric_id": case["numeric_id"],
        "adf_file": artifact["adf_file"],
        "adf_sha256": artifact["sha256"]["adf"],
        "payload_file": artifact["payload_file"],
        "payload_sha256": artifact["sha256"]["payload"],
    }
    if manifest["suite"] != expected_suite:
        raise ValueError(f"{manifest_path}: suite identity mismatch")

    expected_inputs = {
        "capture.toml": PROFILES[profile]["configuration_sha256"],
        "firmware.rom": PROFILES[profile]["firmware_sha256"],
        "stimulus.adf": artifact["sha256"]["adf"],
        "stimulus.bin": artifact["sha256"]["payload"],
        "suite-v1.json": suite_sha256,
    }
    if manifest["inputs"] != expected_inputs:
        raise ValueError(f"{manifest_path}: input manifest mismatch")
    for name, expected_sha256 in expected_inputs.items():
        if sha256_file(run_dir / name) != expected_sha256:
            raise ValueError(f"{run_dir / name}: captured input hash mismatch")
    return manifest


def load_run(
    capture_root: Path,
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    suite_sha256: str,
) -> tuple[list[Image.Image], int, list[int], dict[str, Any]]:
    run_dir = capture_root / f"{profile}-{case['id']}"
    manifest = validate_capture_manifest(
        run_dir,
        profile,
        case,
        artifact,
        suite_sha256,
    )

    log_text = (run_dir / "run.log").read_text(encoding="utf-8")
    if PROFILES[profile]["log_marker"] not in log_text:
        raise ValueError(f"{run_dir}: machine identity is absent from capture log")
    if "Copperline services" in log_text:
        raise ValueError(f"{run_dir}: synthetic services expansion was present")

    break_fields = [int(value) for value in BREAK_RE.findall(log_text)]
    if break_fields != [390]:
        raise ValueError(f"{run_dir}: unexpected debugger break fields {break_fields}")

    memory_matches = MEMORY_RE.findall(log_text)
    if len(memory_matches) != 1:
        raise ValueError(f"{run_dir}: expected one ready-record dump")
    words = [int(word, 16) for word in memory_matches[0].split()]
    if words[:4] != [0x4842, 0x4C4B, case["numeric_id"], 1]:
        raise ValueError(f"{run_dir}: ready record does not match case")
    ready_probe_field = (words[4] << 16) | words[5]
    if ready_probe_field < 8:
        raise ValueError(f"{run_dir}: ready field counter is below eight")

    frame_pairs = [
        (int(index), int(field)) for index, field in FRAME_RE.findall(log_text)
    ]
    if frame_pairs != list(enumerate(CAPTURE_FIELDS)):
        raise ValueError(f"{run_dir}: unexpected captured fields {frame_pairs}")

    counter_by_field = {
        int(field): int(counter) for counter, field in COUNTER_RE.findall(log_text)
    }
    live_probe_fields = [counter_by_field[field] for field in CAPTURE_FIELDS]
    if any(counter == 0 for counter in live_probe_fields):
        raise ValueError(f"{run_dir}: probe field counter cannot be decremented")
    # Copperline increments its producer field label, promotes the field that
    # just completed, and then logs the probe's live counter update in the new
    # field. The dumped pixels therefore correspond to the preceding counter.
    probe_fields = [counter - 1 for counter in live_probe_fields]
    if probe_fields != PROFILES[profile]["probe_fields"]:
        raise ValueError(f"{run_dir}: unexpected probe field counters {probe_fields}")

    frame_paths = sorted((run_dir / "frames").glob("frame-*.png"))
    if len(frame_paths) != 3:
        raise ValueError(f"{run_dir}: expected three PNG frames")
    frames = [Image.open(path).convert("RGBA") for path in frame_paths]
    if any(image.size != (WIDTH, HEIGHT) for image in frames):
        raise ValueError(f"{run_dir}: unexpected frame geometry")

    decoded = [image.tobytes() for image in frames]
    if len(set(decoded)) != 1:
        raise ValueError(f"{run_dir}: adjacent fields are not byte-identical")

    runs = [black_runs(image) for image in frames]
    expected_runs = EXPECTED_RUNS[case["id"]]
    if any(observed != expected_runs for observed in runs):
        raise ValueError(
            f"{run_dir}: black runs {runs} do not match audited result {expected_runs}"
        )

    guard_word = case["line_geometry"]["guard_color_word"]
    guard_rgb = tuple(int(component, 16) * 17 for component in guard_word[2:])
    allowed_pixels = {(0, 0, 0, 255), (*guard_rgb, 255)}
    for image in frames:
        rgba = image.tobytes()
        observed_pixels = {
            tuple(rgba[index : index + 4]) for index in range(0, len(rgba), 4)
        }
        if not observed_pixels.issubset(allowed_pixels):
            raise ValueError(
                f"{run_dir}: frame contains pixels outside guard and black"
            )

    return frames, ready_probe_field, probe_fields, manifest


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
        if packaged.n_frames != len(frames):
            raise ValueError(
                f"{path}: packaged APNG has {packaged.n_frames} frames, "
                f"expected {len(frames)}"
            )
        decoded = [
            frame.convert("RGBA").tobytes()
            for frame in ImageSequence.Iterator(packaged)
        ]
    original = [frame.tobytes() for frame in frames]
    if decoded != original:
        raise ValueError(f"{path}: packaged APNG does not reproduce source frames")
    return sha256_bytes(b"".join(decoded))


def make_record(
    profile: str,
    case: dict[str, Any],
    artifact: dict[str, Any],
    apng_name: str,
    apng_sha256: str,
    decoded_sha256: str,
    ready_probe_field: int,
    probe_fields: list[int],
    capture_manifest: dict[str, Any],
    run_log_name: str,
    run_log_sha256: str,
    manifest_name: str,
    manifest_sha256: str,
) -> dict[str, Any]:
    profile_data = PROFILES[profile]
    start, stop = SEMANTIC_EDGES[case["id"]]
    starts = [start] * 3
    stops = [stop] * 3
    expected_runs = EXPECTED_RUNS[case["id"]]
    run_text = ", ".join(
        f"[{run_start}, {run_stop})" for run_start, run_stop in expected_runs
    )
    if not run_text:
        run_text = "none"

    notes = [
        (
            f"Sample row {SAMPLE_ROW} contained black runs {run_text} in all "
            "three byte-identical adjacent fields."
        ),
        (
            f"The ready record held probe field counter {ready_probe_field}. "
            f"Copperline dump labels {CAPTURE_FIELDS} contain the fields that "
            f"just completed at those boundaries, with probe field counters "
            f"{probe_fields}."
        ),
        (
            "The beam-to-sample mapping came from an audited producer path; "
            "no image alignment search was performed."
        ),
        (
            f"Raw run log ../logs/{run_log_name} has SHA-256 "
            f"{run_log_sha256}."
        ),
        (
            f"Capture-time manifest ../manifests/{manifest_name} has SHA-256 "
            f"{manifest_sha256}."
        ),
    ]
    if case["id"] in GATE_NOTES:
        notes.append(GATE_NOTES[case["id"]])

    active_interval = start is not None
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
            "product": "Copperline",
            "version": "0.13.0",
            "revision": PRODUCER_REVISION,
            "source_url": "https://github.com/CopperlineHQ/Copperline",
            "implementation_family": "Copperline",
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
                f"./capture.sh {profile} {case['id']} "
                "<Copperline-eec5806778dab8b60f3b05fa7ab2428e4e18b073> "
                "<suite-1.0.1-directory> <firmware-matching-recorded-sha256> "
                "<fresh-output-directory> <operator>"
            ),
            "configuration_sha256": profile_data["configuration_sha256"],
            "ready_rule": {
                "record_address": "0x0002ff00",
                "magic": "HBLK",
                "case_number": case["numeric_id"],
                "field_counter_minimum": 8,
                "byte_order": "big-endian",
            },
            "ready_observed_field": 390,
            "settle_fields": 10,
            "captured_fields": CAPTURE_FIELDS,
            "adjacent_field_stability": "confirmed",
        },
        "source_capture": {
            "method": "Copperline raw headless frame dump, packaged as APNG",
            "width": WIDTH,
            "height": HEIGHT,
            "pixel_format": "RGBA8888, tightly packed, row-major",
            "stride_bytes": WIDTH * 4,
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
                "sample_beam_line": 128,
                "sample_row": SAMPLE_ROW,
                "horizontal_origin_sample": -196,
                "horizontal_samples_per_register_increment_numerator": 4,
                "horizontal_samples_per_register_increment_denominator": 1,
                "phase_numerator": 0,
                "phase_denominator": 1,
            },
            "crop": {"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT},
            "field_handling": "bob",
            "color_conversion": "PNG RGBA8 decoded without colour management",
            "alignment_search": False,
        },
        "observations": {
            "guard_color_word": case["line_geometry"]["guard_color_word"],
            "blank_start_samples": starts,
            "blank_stop_samples": stops,
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
            "operator": capture_manifest["capture"]["operator"],
            "capture_date": capture_manifest["capture"]["captured_at_utc"][:10],
            "host": capture_manifest["capture"]["host"],
            "classification": "software-derived",
        },
    }


def package(capture_root: Path, suite_dir: Path, output_root: Path) -> None:
    suite_path = suite_dir / "suite-v1.json"
    suite = json.loads(suite_path.read_text(encoding="utf-8"))
    if suite["suite"]["id"] != SUITE_ID or suite["suite"]["version"] != SUITE_VERSION:
        raise ValueError("suite identity does not match this reference package")
    suite_sha256 = sha256_file(suite_path)

    case_by_id = {case["id"]: case for case in suite["cases"]}
    artifact_by_id = {artifact["case_id"]: artifact for artifact in suite["artifacts"]}
    captures_dir = output_root / "captures"
    records_dir = output_root / "records"
    logs_dir = output_root / "logs"
    manifests_dir = output_root / "manifests"
    captures_dir.mkdir(parents=True, exist_ok=True)
    records_dir.mkdir(parents=True, exist_ok=True)
    logs_dir.mkdir(parents=True, exist_ok=True)
    manifests_dir.mkdir(parents=True, exist_ok=True)
    packaged_runs: list[dict[str, Any]] = []

    for profile in PROFILES:
        config_path = output_root / f"{profile}.toml"
        if sha256_file(config_path) != PROFILES[profile]["configuration_sha256"]:
            raise ValueError(f"{config_path}: configuration hash changed")
        for case_id in CASE_IDS:
            case = case_by_id[case_id]
            artifact = artifact_by_id[case_id]
            adf_sha256 = sha256_file(suite_dir / artifact["adf_file"])
            if adf_sha256 != artifact["sha256"]["adf"]:
                raise ValueError(f"{artifact['adf_file']}: ADF hash mismatch")
            if (
                sha256_file(suite_dir / artifact["payload_file"])
                != artifact["sha256"]["payload"]
            ):
                raise ValueError(f"{artifact['payload_file']}: payload hash mismatch")

            frames, ready_probe_field, probe_fields, capture_manifest = load_run(
                capture_root,
                profile,
                case,
                artifact,
                suite_sha256,
            )
            stem = f"{profile}--{case_id}"
            run_dir = capture_root / f"{profile}-{case_id}"
            run_log_name = f"{stem}.log"
            manifest_name = f"{stem}.json"
            run_log_path = logs_dir / run_log_name
            manifest_path = manifests_dir / manifest_name
            shutil.copyfile(run_dir / "run.log", run_log_path)
            shutil.copyfile(
                run_dir / "capture-manifest.json",
                manifest_path,
            )
            run_log_sha256 = sha256_file(run_log_path)
            manifest_sha256 = sha256_file(manifest_path)

            apng_path = captures_dir / f"{stem}.apng"
            decoded_sha256 = write_apng(apng_path, frames)
            apng_sha256 = sha256_file(apng_path)
            record = make_record(
                profile,
                case,
                artifact,
                apng_path.name,
                apng_sha256,
                decoded_sha256,
                ready_probe_field,
                probe_fields,
                capture_manifest,
                run_log_name,
                run_log_sha256,
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
                    "capture_manifest_file": f"manifests/{manifest_name}",
                    "capture_manifest_sha256": manifest_sha256,
                    "run_log_file": f"logs/{run_log_name}",
                    "run_log_sha256": run_log_sha256,
                    "capture_file": f"captures/{apng_path.name}",
                    "capture_sha256": apng_sha256,
                    "decoded_pixel_sha256": decoded_sha256,
                    "record_file": f"records/{record_path.name}",
                    "record_sha256": sha256_file(record_path),
                }
            )
            print(stem)

    expected_stems = {
        f"{profile}--{case_id}" for profile in PROFILES for case_id in CASE_IDS
    }
    directory_contracts = [
        (captures_dir, ".apng"),
        (records_dir, ".json"),
        (logs_dir, ".log"),
        (manifests_dir, ".json"),
    ]
    for directory, suffix in directory_contracts:
        actual_stems = {path.stem for path in directory.glob(f"*{suffix}")}
        if actual_stems != expected_stems:
            raise ValueError(f"{directory}: stale or missing packaged files")

    package_manifest = {
        "schema_version": "1.0.0",
        "suite": {
            "id": SUITE_ID,
            "version": SUITE_VERSION,
            "source_revision": suite["suite"]["source_revision"],
            "manifest_sha256": suite_sha256,
        },
        "producer": {
            "product": "Copperline",
            "version": "0.13.0",
            "revision": PRODUCER_REVISION,
            "binary_sha256": PRODUCER_BINARY_SHA256,
        },
        "capture_tools": capture_tool_hashes(),
        "configurations": {
            profile: data["configuration_sha256"]
            for profile, data in PROFILES.items()
        },
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
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
