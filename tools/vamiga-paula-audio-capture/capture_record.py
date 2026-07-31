#!/usr/bin/env python3
"""Build provenance and capture-v1 records for the vAmiga Paula adapter."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import shutil
import struct
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
EXPECTED_SUITE_ID = "org.198x.amiga.paula-audio"
EXPECTED_SUITE_VERSION = "1.0.0"
EXPECTED_ROM_BYTES = 256 * 1024
EXPECTED_ROM_SHA256 = "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53"
EXPECTED_PRODUCER_VERSION = "4.4b12"
EXPECTED_PRODUCER_REVISION = "60fd1e6b69dcd77c9f44d1291bd37ec715362ab0"
SAMPLE_RATE_HZ = 48_000


class CaptureError(RuntimeError):
    """A capture input or result violated the evidence contract."""


def utc_now() -> str:
    return datetime.now(UTC).isoformat(timespec="seconds")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CaptureError(f"cannot read JSON {path}: {error}") from error
    if not isinstance(value, dict):
        raise CaptureError(f"expected a JSON object in {path}")
    return value


def write_json(path: Path, value: Any) -> None:
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    os.replace(temporary, path)


def load_suite(path: Path) -> dict[str, Any]:
    suite = read_json(path)
    if suite.get("schema_version") != "1.0.0":
        raise CaptureError("suite manifest schema is not 1.0.0")
    identity = suite.get("suite")
    if not isinstance(identity, dict):
        raise CaptureError("suite manifest has no suite identity")
    if identity.get("id") != EXPECTED_SUITE_ID:
        raise CaptureError("unexpected suite identifier")
    if identity.get("version") != EXPECTED_SUITE_VERSION:
        raise CaptureError("unexpected suite version")
    cases = suite.get("cases")
    artifacts = suite.get("artifacts")
    if not isinstance(cases, list) or not cases:
        raise CaptureError("suite has no cases")
    if not isinstance(artifacts, list) or len(artifacts) != len(cases):
        raise CaptureError("suite artifact matrix does not match its cases")
    return suite


def case_and_artifact(
    suite: dict[str, Any], case_id: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    cases = [
        case
        for case in suite["cases"]
        if isinstance(case, dict) and case.get("id") == case_id
    ]
    artifacts = [
        artifact
        for artifact in suite["artifacts"]
        if isinstance(artifact, dict) and artifact.get("case_id") == case_id
    ]
    if len(cases) != 1 or len(artifacts) != 1:
        raise CaptureError(f"suite does not contain exactly one {case_id!r} case")
    return cases[0], artifacts[0]


def checked_artifact(
    suite_dir: Path, artifact: dict[str, Any], file_key: str, hash_key: str
) -> Path:
    name = artifact.get(file_key)
    hashes = artifact.get("sha256")
    if not isinstance(name, str) or Path(name).name != name:
        raise CaptureError(f"unsafe artifact file name for {file_key}")
    if not isinstance(hashes, dict) or not isinstance(hashes.get(hash_key), str):
        raise CaptureError(f"missing artifact hash for {hash_key}")
    path = suite_dir / name
    if not path.is_file():
        raise CaptureError(f"missing suite artifact {path}")
    if sha256(path) != hashes[hash_key]:
        raise CaptureError(f"suite artifact hash mismatch: {path}")
    return path


def require_file(path: Path, description: str) -> None:
    if not path.is_file():
        raise CaptureError(f"missing {description}: {path}")


def snapshot_inputs(
    *,
    firmware: Path,
    suite_manifest: Path,
    adf: Path,
    payload: Path,
    binary: Path,
    build_record: Path,
) -> dict[str, Any]:
    tools = {
        name: sha256(SCRIPT_DIR / name)
        for name in ("CMakeLists.txt", "README.md", "capture.sh", "capture_record.py", "main.cpp")
    }
    return {
        "adapter_binary": {
            "bytes": binary.stat().st_size,
            "sha256": sha256(binary),
        },
        "adapter_tools": tools,
        "build_record": {
            "bytes": build_record.stat().st_size,
            "sha256": sha256(build_record),
        },
        "firmware": {
            "bytes": firmware.stat().st_size,
            "sha256": sha256(firmware),
        },
        "staged_artifacts": {
            adf.name: {"bytes": adf.stat().st_size, "sha256": sha256(adf)},
            payload.name: {
                "bytes": payload.stat().st_size,
                "sha256": sha256(payload),
            },
            suite_manifest.name: {
                "bytes": suite_manifest.stat().st_size,
                "sha256": sha256(suite_manifest),
            },
        },
    }


def parse_float_wav(path: Path) -> tuple[list[float], list[float]]:
    data = path.read_bytes()
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise CaptureError("source capture is not RIFF/WAVE")
    declared_size = struct.unpack_from("<I", data, 4)[0]
    if declared_size + 8 != len(data):
        raise CaptureError("source WAVE has an inconsistent RIFF size")

    chunks: dict[bytes, bytes] = {}
    offset = 12
    while offset + 8 <= len(data):
        chunk_id = data[offset : offset + 4]
        chunk_size = struct.unpack_from("<I", data, offset + 4)[0]
        start = offset + 8
        end = start + chunk_size
        if end > len(data):
            raise CaptureError("source WAVE contains a truncated chunk")
        if chunk_id in chunks:
            raise CaptureError(f"source WAVE repeats chunk {chunk_id!r}")
        chunks[chunk_id] = data[start:end]
        offset = end + (chunk_size & 1)
    if offset != len(data):
        raise CaptureError("source WAVE has trailing bytes")

    format_chunk = chunks.get(b"fmt ")
    fact_chunk = chunks.get(b"fact")
    sample_data = chunks.get(b"data")
    if format_chunk is None or fact_chunk is None or sample_data is None:
        raise CaptureError("source WAVE lacks fmt, fact, or data")
    if len(format_chunk) != 18:
        raise CaptureError("source WAVE fmt chunk is not the recorded 18-byte form")
    (
        format_tag,
        channels,
        sample_rate,
        byte_rate,
        block_align,
        bits_per_sample,
        extension_size,
    ) = struct.unpack("<HHIIHHH", format_chunk)
    if (
        format_tag != 3
        or channels != 2
        or sample_rate != SAMPLE_RATE_HZ
        or byte_rate != SAMPLE_RATE_HZ * 8
        or block_align != 8
        or bits_per_sample != 32
        or extension_size != 0
    ):
        raise CaptureError("source WAVE format does not match the capture contract")
    if len(sample_data) % block_align != 0:
        raise CaptureError("source WAVE ends with an incomplete stereo frame")
    frames = len(sample_data) // block_align
    if len(fact_chunk) != 4 or struct.unpack("<I", fact_chunk)[0] != frames:
        raise CaptureError("source WAVE fact count does not match its data")
    if frames < 100:
        raise CaptureError("source WAVE capture window is too short")

    values = struct.unpack(f"<{frames * 2}f", sample_data)
    if not all(math.isfinite(value) for value in values):
        raise CaptureError("source WAVE contains a non-finite sample")
    return list(values[0::2]), list(values[1::2])


def ac_rms(samples: list[float]) -> float:
    mean = math.fsum(samples) / len(samples)
    return math.sqrt(
        math.fsum((sample - mean) ** 2 for sample in samples) / len(samples)
    )


def fundamental_hz(samples: list[float]) -> float | None:
    if ac_rms(samples) < 1.0 / 32_768.0:
        return None
    mean = math.fsum(samples) / len(samples)
    crossings = [
        index
        for index in range(1, len(samples))
        if samples[index - 1] - mean <= 0.0 and samples[index] - mean > 0.0
    ]
    if len(crossings) < 2 or crossings[-1] <= crossings[0]:
        return None
    return SAMPLE_RATE_HZ * (len(crossings) - 1) / (crossings[-1] - crossings[0])


def analyze_capture(
    wav: Path, case: dict[str, Any], output_root: Path
) -> dict[str, Any]:
    left, right = parse_float_wav(wav)
    left_rms = ac_rms(left)
    right_rms = ac_rms(right)
    left_hz = fundamental_hz(left)
    right_hz = fundamental_hz(right)

    channel = case.get("channel")
    if not isinstance(channel, int) or channel not in range(4):
        raise CaptureError("case channel is not in 0..3")
    expected_dominant = "right" if channel in (0, 3) else "left"
    dominant_rms = right_rms if expected_dominant == "right" else left_rms
    silent_rms = left_rms if expected_dominant == "right" else right_rms
    dominant_hz = right_hz if expected_dominant == "right" else left_hz
    if dominant_rms <= 0.001:
        raise CaptureError("expected dominant output is unexpectedly quiet")
    if silent_rms >= 1.0 / 32_768.0:
        raise CaptureError("hard-stereo capture leaked into the inactive output")
    if dominant_hz is None:
        raise CaptureError("dominant output has no measurable fundamental")

    period = case.get("period_cck")
    if not isinstance(period, int) or period <= 0:
        raise CaptureError("case has no valid Paula period")
    nominal_hz = (28_375_160 / 8) / (2 * period)
    if abs(dominant_hz - nominal_hz) / nominal_hz >= 0.01:
        raise CaptureError(
            f"measured fundamental {dominant_hz:.6f} Hz differs from "
            f"nominal {nominal_hz:.6f} Hz"
        )

    if silent_rms == 0.0:
        dominance_db = None
    else:
        dominance_db = 20.0 * math.log10(dominant_rms / silent_rms)

    amplitude_ratio: dict[str, Any] | None = None
    comparison = case.get("comparison")
    if isinstance(comparison, dict):
        reference_id = comparison.get("case_id")
        if not isinstance(reference_id, str):
            raise CaptureError("case comparison lacks a reference case")
        reference_path = output_root / reference_id / "capture-record.json"
        if reference_path.is_file():
            reference = read_json(reference_path)
            reference_observations = reference.get("observations")
            if not isinstance(reference_observations, dict):
                raise CaptureError("reference case has no observations")
            reference_rms = reference_observations.get("rms")
            reference_dominant = reference_observations.get("dominant_channel")
            if not isinstance(reference_rms, dict) or reference_dominant not in (
                "left",
                "right",
            ):
                raise CaptureError("reference case has no dominant RMS")
            denominator = reference_rms.get(reference_dominant)
            if not isinstance(denominator, (int, float)) or denominator <= 0:
                raise CaptureError("reference case dominant RMS is invalid")
            amplitude_ratio = {
                "metric": "dominant-channel AC RMS",
                "reference_case_id": reference_id,
                "value": dominant_rms / float(denominator),
            }

    return {
        "status": "observed",
        "analysis_window_seconds": {
            "start": 0.0,
            "end": len(left) / SAMPLE_RATE_HZ,
        },
        "fundamental_hz": {"left": left_hz, "right": right_hz},
        "rms": {"left": left_rms, "right": right_rms},
        "dominant_channel": expected_dominant,
        "channel_dominance_db": dominance_db,
        "amplitude_ratio": amplitude_ratio,
        "analysis_procedure": (
            "Decode the complete IEEE-754 stereo WAVE; subtract each channel "
            "mean before RMS; measure the fundamental from the first and last "
            "positive-going mean crossing; apply no crop, gain, remap, filter, "
            "or sample conversion."
        ),
    }


def validate_adapter_result(
    result: dict[str, Any], case: dict[str, Any]
) -> None:
    if result.get("schema_version") != "1.0.0":
        raise CaptureError("adapter result schema is not 1.0.0")
    if result.get("producer_version") != EXPECTED_PRODUCER_VERSION:
        raise CaptureError("adapter was not built from vAmiga 4.4b12")
    if result.get("case_id") != case.get("id"):
        raise CaptureError("adapter result case does not match the suite")
    ready = result.get("ready_record")
    if not isinstance(ready, dict):
        raise CaptureError("adapter result has no ready record")
    if ready.get("field_counter") != 8:
        raise CaptureError("adapter did not begin after guest field 8")
    if result.get("captured_guest_fields") != [9, 10, 11]:
        raise CaptureError("adapter did not capture guest fields 9, 10, and 11")
    emulator_frames = result.get("captured_emulator_frames")
    if (
        not isinstance(emulator_frames, list)
        or len(emulator_frames) != 3
        or emulator_frames[1] != emulator_frames[0] + 1
        or emulator_frames[2] != emulator_frames[1] + 1
    ):
        raise CaptureError("adapter did not capture adjacent vAmiga fields")
    field_frames = result.get("field_sample_frames")
    sample_frames = result.get("sample_frames")
    if (
        not isinstance(field_frames, list)
        or len(field_frames) != 3
        or not all(isinstance(value, int) and value > 0 for value in field_frames)
        or not isinstance(sample_frames, int)
        or sum(field_frames) != sample_frames
    ):
        raise CaptureError("adapter audio frame counts are inconsistent")
    statistics = result.get("audio_statistics")
    if (
        not isinstance(statistics, dict)
        or statistics.get("buffer_underflows") != 0
        or statistics.get("buffer_overflows") != 0
    ):
        raise CaptureError("adapter reported an audio-buffer fault")


def command_list_cases(args: argparse.Namespace) -> None:
    suite = load_suite(args.suite)
    for case in suite["cases"]:
        if not isinstance(case, dict) or not isinstance(case.get("id"), str):
            raise CaptureError("suite contains an invalid case")
        print(case["id"])


def command_require_case(args: argparse.Namespace) -> None:
    suite = load_suite(args.suite)
    case_and_artifact(suite, args.case_id)


def command_build_record(args: argparse.Namespace) -> None:
    binary = args.binary.resolve()
    build_log = args.build_log.resolve()
    require_file(binary, "adapter binary")
    require_file(build_log, "adapter build log")
    if args.revision != EXPECTED_PRODUCER_REVISION:
        raise CaptureError("unexpected vAmiga source revision")

    def version(command: list[str]) -> str:
        try:
            output = subprocess.run(
                command,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            ).stdout
        except (OSError, subprocess.CalledProcessError) as error:
            raise CaptureError(f"cannot identify build tool {command[0]}: {error}") from error
        return output.splitlines()[0].strip()

    record = {
        "schema_version": "1.0.0",
        "producer": {
            "product": "vAmiga",
            "version": EXPECTED_PRODUCER_VERSION,
            "revision": args.revision,
            "source_url": "https://github.com/dirkwhoffmann/vAmiga",
            "source_tree_clean": True,
        },
        "adapter": {
            "binary_bytes": binary.stat().st_size,
            "binary_sha256": sha256(binary),
            "source_sha256": {
                name: sha256(SCRIPT_DIR / name)
                for name in (
                    "CMakeLists.txt",
                    "README.md",
                    "capture.sh",
                    "capture_record.py",
                    "main.cpp",
                )
            },
        },
        "build": {
            "type": "Release",
            "cmake": version(["cmake", "--version"]),
            "compiler": version(["c++", "--version"]),
            "log_file": build_log.name,
            "log_sha256": sha256(build_log),
        },
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "record_created_at": utc_now(),
    }
    write_json(args.output, record)


def command_capture(args: argparse.Namespace) -> None:
    binary = args.binary.resolve()
    suite_dir = args.suite_dir.resolve()
    firmware = args.firmware.resolve()
    output_root = args.output_root.resolve()
    build_record_source = args.build_record.resolve()
    require_file(binary, "adapter binary")
    require_file(suite_dir / "suite-v1.json", "suite manifest")
    require_file(firmware, "Kickstart firmware")
    require_file(build_record_source, "producer build record")
    if firmware.stat().st_size != EXPECTED_ROM_BYTES or sha256(firmware) != EXPECTED_ROM_SHA256:
        raise CaptureError("Kickstart image does not match revision 34.005")
    if args.revision != EXPECTED_PRODUCER_REVISION:
        raise CaptureError("unexpected vAmiga source revision")
    if not args.operator.strip():
        raise CaptureError("operator must not be empty")

    suite_source = suite_dir / "suite-v1.json"
    suite = load_suite(suite_source)
    case, artifact = case_and_artifact(suite, args.case_id)
    adf_source = checked_artifact(suite_dir, artifact, "adf_file", "adf")
    payload_source = checked_artifact(
        suite_dir, artifact, "payload_file", "payload"
    )
    if adf_source.stat().st_size != artifact.get("adf_bytes"):
        raise CaptureError("ADF size does not match the suite manifest")
    if payload_source.stat().st_size != artifact.get("payload_bytes"):
        raise CaptureError("payload size does not match the suite manifest")

    run_dir = output_root / args.case_id
    if run_dir.exists():
        raise CaptureError(f"refusing to overwrite capture run {run_dir}")
    inputs = run_dir / "inputs"
    inputs.mkdir(parents=True)

    staged_suite = inputs / "suite-v1.json"
    staged_adf = inputs / adf_source.name
    staged_payload = inputs / payload_source.name
    build_record = run_dir / "producer-build.json"
    shutil.copyfile(suite_source, staged_suite)
    shutil.copyfile(adf_source, staged_adf)
    shutil.copyfile(payload_source, staged_payload)
    shutil.copyfile(build_record_source, build_record)

    before = snapshot_inputs(
        firmware=firmware,
        suite_manifest=staged_suite,
        adf=staged_adf,
        payload=staged_payload,
        binary=binary,
        build_record=build_record,
    )
    write_json(run_dir / "inputs-before.json", before)

    wav = run_dir / "capture.wav"
    config = run_dir / "configuration.retrosh"
    adapter_result_path = run_dir / "adapter-result.json"
    producer_log = run_dir / "producer.log"
    sample = case.get("sample")
    if not isinstance(sample, dict):
        raise CaptureError("case has no sample definition")
    command = [
        str(binary),
        str(firmware),
        str(staged_adf),
        str(wav),
        str(config),
        str(adapter_result_path),
        str(case["id"]),
        str(case["numeric_id"]),
        str(case["channel"]),
        str(case["period_cck"]),
        str(case["volume"]),
        str(sample["word"]),
        str(sample["words"]),
        str(case["serial_identity"]),
    ]
    captured_at = utc_now()
    with producer_log.open("w", encoding="utf-8", newline="\n") as log:
        result = subprocess.run(
            command,
            stdout=log,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    if result.returncode != 0:
        raise CaptureError(
            f"vAmiga adapter failed for {args.case_id}; see {producer_log}"
        )
    for path, description in (
        (wav, "source WAVE"),
        (config, "exported configuration"),
        (adapter_result_path, "adapter result"),
    ):
        require_file(path, description)

    adapter_result = read_json(adapter_result_path)
    validate_adapter_result(adapter_result, case)
    observations = analyze_capture(wav, case, output_root)

    after = snapshot_inputs(
        firmware=firmware,
        suite_manifest=staged_suite,
        adf=staged_adf,
        payload=staged_payload,
        binary=binary,
        build_record=build_record,
    )
    write_json(run_dir / "inputs-after.json", after)
    if after != before:
        raise CaptureError("capture inputs changed during execution")

    notes = (
        f"Host: {platform.platform()}, {platform.machine()}. "
        f"Adapter binary SHA-256 {sha256(binary)}; adapter-result SHA-256 "
        f"{sha256(adapter_result_path)}; producer-log SHA-256 "
        f"{sha256(producer_log)}; producer-build SHA-256 {sha256(build_record)}. "
        "The source checkout was clean at the recorded revision. The WAVE "
        "retains vAmiga API left/right order with hard stereo and no channel "
        "remapping. Firmware and the compiled producer are not retained in "
        "the reference package."
    )
    capture_record = {
        "schema_version": "1.0.0",
        "suite_id": EXPECTED_SUITE_ID,
        "suite_version": EXPECTED_SUITE_VERSION,
        "case_id": case["id"],
        "artifact": {
            "adf_file": f"inputs/{staged_adf.name}",
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": f"inputs/{staged_payload.name}",
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "producer": {
            "kind": "software-emulator",
            "product": "vAmiga",
            "version": adapter_result["producer_version"],
            "revision": args.revision,
            "source_url": "https://github.com/dirkwhoffmann/vAmiga",
            "implementation_family": "vAmiga",
        },
        "machine": {
            "model": "Amiga 500",
            "cpu": "Motorola 68000",
            "chipset": "OCS",
            "region": "PAL",
            "ram_bytes": 1024 * 1024,
            "firmware": {
                "revision": "Kickstart 1.3 revision 34.005",
                "sha256": EXPECTED_ROM_SHA256,
            },
        },
        "execution": {
            "cold_boot": True,
            "command_or_procedure": (
                "tools/vamiga-paula-audio-capture/capture.sh "
                f"{case['id']} <vAmiga-source-at-{args.revision}> "
                "<suite-1.0.0-dist> <firmware-matching-recorded-sha256> "
                "<fresh-output-root> <operator>"
            ),
            "configuration_sha256": sha256(config),
            "ready_rule": {
                "record_address": "0x0002ff00",
                "magic": "PAUD",
                "case_number": case["numeric_id"],
                "field_counter_minimum": 8,
                "byte_order": "big-endian",
            },
            "ready_observed_field": adapter_result["ready_record"]["field_counter"],
            "settle_fields": 8,
            "captured_fields": adapter_result["captured_guest_fields"],
        },
        "source_capture": {
            "domain": "modelled-analogue-output",
            "method": (
                "vAmiga VACore AudioPortAPI::copyInterleaved, drained after "
                "each complete VSYNC-driven field while suspended"
            ),
            "sample_rate_hz": SAMPLE_RATE_HZ,
            "channels": 2,
            "sample_format": "IEEE-754 binary32 little-endian WAVE",
            "filtering": (
                "vAmiga A500 modelled filter pipeline; the probe disables "
                "the switchable LED low-pass stage through CIA-A"
            ),
            "resampling": "vAmiga linear interpolation to 48000 Hz; ASR disabled",
            "automatic_gain_control": False,
            "channel_remapping": False,
            "file_name": "capture.wav",
            "file_sha256": sha256(wav),
        },
        "observations": observations,
        "provenance": {
            "captured_at": captured_at,
            "record_created_at": utc_now(),
            "operator": args.operator,
            "notes": notes,
        },
    }
    write_json(run_dir / "capture-record.json", capture_record)
    print(
        f"captured {args.case_id}: "
        f"left_rms={observations['rms']['left']:.9f} "
        f"right_rms={observations['rms']['right']:.9f}"
    )


def command_verify_suite(args: argparse.Namespace) -> None:
    suite = load_suite(args.suite)
    records: dict[str, dict[str, Any]] = {}
    for case in suite["cases"]:
        case_id = case.get("id")
        if not isinstance(case_id, str):
            raise CaptureError("suite contains an invalid case")
        record = read_json(args.output_root / case_id / "capture-record.json")
        if record.get("case_id") != case_id:
            raise CaptureError(f"capture record identity mismatch for {case_id}")
        records[case_id] = record

    channel_0_full = records["channel-0-full"]["observations"]
    channel_1_full = records["channel-1-full"]["observations"]
    channel_0_half = records["channel-0-half"]["observations"]
    full_0 = float(channel_0_full["rms"]["right"])
    full_1 = float(channel_1_full["rms"]["left"])
    half_0 = float(channel_0_half["rms"]["right"])
    if abs(full_0 - full_1) / full_0 >= 0.01:
        raise CaptureError("equivalent full-volume channels differ by at least 1%")
    if abs(half_0 / full_0 - 0.5) >= 0.01:
        raise CaptureError("channel-0 half/full RMS ratio differs from 0.5")
    ratio = channel_0_half.get("amplitude_ratio")
    if (
        not isinstance(ratio, dict)
        or ratio.get("reference_case_id") != "channel-0-full"
        or abs(float(ratio.get("value", 0.0)) - 0.5) >= 0.01
    ):
        raise CaptureError("paired amplitude-ratio record is incomplete")
    print(
        "verified suite: "
        f"ch0_full={full_0:.9f} ch1_full={full_1:.9f} "
        f"half_full={half_0 / full_0:.9f}"
    )


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)

    list_cases = commands.add_parser("list-cases")
    list_cases.add_argument("suite", type=Path)
    list_cases.set_defaults(handler=command_list_cases)

    require_case = commands.add_parser("require-case")
    require_case.add_argument("suite", type=Path)
    require_case.add_argument("case_id")
    require_case.set_defaults(handler=command_require_case)

    build_record = commands.add_parser("build-record")
    build_record.add_argument("revision")
    build_record.add_argument("binary", type=Path)
    build_record.add_argument("build_log", type=Path)
    build_record.add_argument("output", type=Path)
    build_record.set_defaults(handler=command_build_record)

    capture = commands.add_parser("capture")
    capture.add_argument("case_id")
    capture.add_argument("binary", type=Path)
    capture.add_argument("suite_dir", type=Path)
    capture.add_argument("firmware", type=Path)
    capture.add_argument("output_root", type=Path)
    capture.add_argument("operator")
    capture.add_argument("revision")
    capture.add_argument("build_record", type=Path)
    capture.set_defaults(handler=command_capture)

    verify = commands.add_parser("verify-suite")
    verify.add_argument("suite", type=Path)
    verify.add_argument("output_root", type=Path)
    verify.set_defaults(handler=command_verify_suite)
    return result


def main() -> int:
    try:
        args = parser().parse_args()
        args.handler(args)
        return 0
    except CaptureError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
