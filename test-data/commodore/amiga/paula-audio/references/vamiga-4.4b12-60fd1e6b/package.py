#!/usr/bin/env python3
"""Package and verify the registered vAmiga Paula-audio capture."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import struct
import sys
from pathlib import Path
from typing import Any


PACKAGE_ROOT = Path(__file__).resolve().parent
CASES = ("channel-0-full", "channel-1-full", "channel-0-half")
PRODUCER_VERSION = "4.4b12"
PRODUCER_REVISION = "60fd1e6b69dcd77c9f44d1291bd37ec715362ab0"
SUITE_ID = "org.198x.amiga.paula-audio"
SUITE_VERSION = "1.0.0"
CONFIG_SHA256 = "1252cb3fc1366e37f09946fd109c0bdd5dcd2ca4f9595a37f1402d54f5a3426e"
CAPTURE_SHA256 = {
    "channel-0-full": "8f6b01df390270f07f4541333793674887ade48cedc96d4d5e02dcfaf779dacf",
    "channel-1-full": "8ccc7956bb6487626b8c510f0800b1ad6f00536f0849620abbce076d9017806d",
    "channel-0-half": "dc728b4075b627a07b1a607ff30ab98bbd23bbfd767a373be8427e7d17c3a2ff",
}
DOMINANT_OUTPUT = {
    "channel-0-full": "right",
    "channel-1-full": "left",
    "channel-0-half": "right",
}


class PackageError(RuntimeError):
    """A raw run or packaged file violated the evidence contract."""


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
        raise PackageError(f"cannot read JSON {path}: {error}") from error
    if not isinstance(value, dict):
        raise PackageError(f"{path}: expected a JSON object")
    return value


def write_json(path: Path, value: Any) -> None:
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    os.replace(temporary, path)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise PackageError(message)


def require_file(path: Path) -> None:
    if not path.is_file():
        raise PackageError(f"missing file: {path}")


def parse_float_wav(path: Path) -> tuple[list[float], list[float]]:
    data = path.read_bytes()
    require(
        len(data) >= 12 and data[:4] == b"RIFF" and data[8:12] == b"WAVE",
        f"{path}: not RIFF/WAVE",
    )
    require(
        struct.unpack_from("<I", data, 4)[0] + 8 == len(data),
        f"{path}: inconsistent RIFF size",
    )

    chunks: dict[bytes, bytes] = {}
    offset = 12
    while offset + 8 <= len(data):
        chunk_id = data[offset : offset + 4]
        size = struct.unpack_from("<I", data, offset + 4)[0]
        start = offset + 8
        end = start + size
        require(end <= len(data), f"{path}: truncated chunk")
        require(chunk_id not in chunks, f"{path}: repeated chunk {chunk_id!r}")
        chunks[chunk_id] = data[start:end]
        offset = end + (size & 1)
    require(offset == len(data), f"{path}: trailing bytes")

    fmt = chunks.get(b"fmt ")
    fact = chunks.get(b"fact")
    samples = chunks.get(b"data")
    require(fmt is not None and len(fmt) == 18, f"{path}: invalid fmt chunk")
    require(fact is not None and len(fact) == 4, f"{path}: invalid fact chunk")
    require(samples is not None, f"{path}: missing data chunk")
    require(
        struct.unpack("<HHIIHHH", fmt)
        == (3, 2, 48_000, 384_000, 8, 32, 0),
        f"{path}: unexpected audio format",
    )
    require(len(samples) % 8 == 0, f"{path}: incomplete stereo frame")
    frames = len(samples) // 8
    require(
        frames == 2_885 and struct.unpack("<I", fact)[0] == frames,
        f"{path}: unexpected frame count",
    )
    values = struct.unpack(f"<{frames * 2}f", samples)
    require(
        all(math.isfinite(value) for value in values),
        f"{path}: non-finite sample",
    )
    return list(values[0::2]), list(values[1::2])


def ac_rms(samples: list[float]) -> float:
    mean = math.fsum(samples) / len(samples)
    return math.sqrt(
        math.fsum((sample - mean) ** 2 for sample in samples) / len(samples)
    )


def validate_observations(
    case_id: str, record: dict[str, Any], capture: Path
) -> dict[str, Any]:
    left, right = parse_float_wav(capture)
    observations = record.get("observations")
    require(isinstance(observations, dict), f"{case_id}: missing observations")
    rms = observations.get("rms")
    require(isinstance(rms, dict), f"{case_id}: missing RMS")
    left_rms = ac_rms(left)
    right_rms = ac_rms(right)
    require(
        math.isclose(float(rms.get("left", -1)), left_rms, abs_tol=1e-15),
        f"{case_id}: left RMS does not match capture",
    )
    require(
        math.isclose(float(rms.get("right", -1)), right_rms, abs_tol=1e-15),
        f"{case_id}: right RMS does not match capture",
    )
    require(
        observations.get("dominant_channel") == DOMINANT_OUTPUT[case_id],
        f"{case_id}: incorrect dominant output",
    )
    inactive = left_rms if DOMINANT_OUTPUT[case_id] == "right" else right_rms
    require(inactive == 0.0, f"{case_id}: inactive output is not silent")
    return observations


def validate_source_record(
    case_id: str, run_dir: Path
) -> tuple[dict[str, Any], dict[str, Any]]:
    for name in (
        "adapter-result.json",
        "capture-record.json",
        "capture.wav",
        "configuration.retrosh",
        "inputs-after.json",
        "inputs-before.json",
        "producer-build.json",
        "producer.log",
    ):
        require_file(run_dir / name)
    require(
        read_json(run_dir / "inputs-before.json")
        == read_json(run_dir / "inputs-after.json"),
        f"{case_id}: capture inputs changed during execution",
    )
    record = read_json(run_dir / "capture-record.json")
    require(record.get("schema_version") == "1.0.0", f"{case_id}: bad schema")
    require(record.get("suite_id") == SUITE_ID, f"{case_id}: bad suite")
    require(
        record.get("suite_version") == SUITE_VERSION,
        f"{case_id}: bad suite version",
    )
    require(record.get("case_id") == case_id, f"{case_id}: bad record identity")
    producer = record.get("producer")
    require(
        isinstance(producer, dict)
        and producer.get("version") == PRODUCER_VERSION
        and producer.get("revision") == PRODUCER_REVISION
        and producer.get("implementation_family") == "vAmiga",
        f"{case_id}: bad producer identity",
    )
    execution = record.get("execution")
    require(
        isinstance(execution, dict)
        and execution.get("ready_observed_field") == 8
        and execution.get("captured_fields") == [9, 10, 11]
        and execution.get("configuration_sha256") == CONFIG_SHA256,
        f"{case_id}: bad execution boundary",
    )
    source = record.get("source_capture")
    require(
        isinstance(source, dict)
        and source.get("domain") == "modelled-analogue-output"
        and source.get("sample_rate_hz") == 48_000
        and source.get("channel_remapping") is False
        and source.get("automatic_gain_control") is False
        and source.get("file_sha256") == CAPTURE_SHA256[case_id],
        f"{case_id}: bad source-capture boundary",
    )
    capture = run_dir / "capture.wav"
    require(
        sha256(capture) == CAPTURE_SHA256[case_id],
        f"{case_id}: source WAVE hash mismatch",
    )
    require(
        sha256(run_dir / "configuration.retrosh") == CONFIG_SHA256,
        f"{case_id}: configuration hash mismatch",
    )
    observations = validate_observations(case_id, record, capture)
    adapter_result = read_json(run_dir / "adapter-result.json")
    require(
        adapter_result.get("producer_version") == PRODUCER_VERSION
        and adapter_result.get("case_id") == case_id
        and adapter_result.get("captured_guest_fields") == [9, 10, 11],
        f"{case_id}: bad adapter result",
    )
    return record, observations


def packaged_record(
    source: dict[str, Any],
    case_id: str,
    run_dir: Path,
) -> dict[str, Any]:
    result = json.loads(json.dumps(source))
    result["artifact"]["adf_file"] = f"{case_id}.adf"
    result["artifact"]["payload_file"] = f"{case_id}.bin"
    result["source_capture"]["file_name"] = f"../captures/{case_id}.wav"
    suffix = (
        f" Packaged configuration ../configs/{case_id}.retrosh has SHA-256 "
        f"{sha256(run_dir / 'configuration.retrosh')}; capture-time manifest "
        f"../manifests/{case_id}.json has SHA-256 "
        f"{sha256(run_dir / 'adapter-result.json')}; producer log "
        f"../logs/{case_id}.log has SHA-256 {sha256(run_dir / 'producer.log')}."
    )
    result["provenance"]["notes"] += suffix
    return result


def copy_new(source: Path, destination: Path) -> None:
    if destination.exists():
        raise PackageError(f"refusing to overwrite {destination}")
    shutil.copyfile(source, destination)


def create_package(capture_root: Path) -> None:
    require_file(capture_root / "producer-build.json")
    require_file(capture_root / "producer-build.log")
    build = read_json(capture_root / "producer-build.json")
    producer = build.get("producer")
    require(
        isinstance(producer, dict)
        and producer.get("version") == PRODUCER_VERSION
        and producer.get("revision") == PRODUCER_REVISION
        and producer.get("source_tree_clean") is True,
        "producer build identity is invalid",
    )
    require(
        build.get("build", {}).get("log_sha256")
        == sha256(capture_root / "producer-build.log"),
        "producer build log hash mismatch",
    )

    for directory in ("captures", "configs", "logs", "manifests", "records"):
        require((PACKAGE_ROOT / directory / "README.md").is_file(), f"missing {directory} README")

    runs: list[dict[str, Any]] = []
    observations_by_case: dict[str, dict[str, Any]] = {}
    for case_id in CASES:
        run_dir = capture_root / case_id
        require(run_dir.is_dir(), f"missing raw run for {case_id}")
        source_record, observations = validate_source_record(case_id, run_dir)
        observations_by_case[case_id] = observations

        capture_file = PACKAGE_ROOT / "captures" / f"{case_id}.wav"
        config_file = PACKAGE_ROOT / "configs" / f"{case_id}.retrosh"
        log_file = PACKAGE_ROOT / "logs" / f"{case_id}.log"
        manifest_file = PACKAGE_ROOT / "manifests" / f"{case_id}.json"
        record_file = PACKAGE_ROOT / "records" / f"{case_id}.json"
        copy_new(run_dir / "capture.wav", capture_file)
        copy_new(run_dir / "configuration.retrosh", config_file)
        copy_new(run_dir / "producer.log", log_file)
        copy_new(run_dir / "adapter-result.json", manifest_file)
        write_json(
            record_file,
            packaged_record(source_record, case_id, run_dir),
        )
        runs.append(
            {
                "case_id": case_id,
                "capture_file": f"captures/{capture_file.name}",
                "capture_sha256": sha256(capture_file),
                "configuration_file": f"configs/{config_file.name}",
                "configuration_sha256": sha256(config_file),
                "manifest_file": f"manifests/{manifest_file.name}",
                "manifest_sha256": sha256(manifest_file),
                "producer_log_file": f"logs/{log_file.name}",
                "producer_log_sha256": sha256(log_file),
                "record_file": f"records/{record_file.name}",
                "record_sha256": sha256(record_file),
            }
        )

    full_0 = float(observations_by_case["channel-0-full"]["rms"]["right"])
    full_1 = float(observations_by_case["channel-1-full"]["rms"]["left"])
    half_0 = float(observations_by_case["channel-0-half"]["rms"]["right"])
    require(abs(full_0 - full_1) / full_0 < 0.01, "full channels differ by at least 1%")
    require(abs(half_0 / full_0 - 0.5) < 0.01, "half/full ratio differs from 0.5")

    copy_new(
        capture_root / "producer-build.json",
        PACKAGE_ROOT / "producer-build-v1.json",
    )
    copy_new(
        capture_root / "producer-build.log",
        PACKAGE_ROOT / "logs" / "producer-build.log",
    )
    package = {
        "schema_version": "1.0.0",
        "suite": {"id": SUITE_ID, "version": SUITE_VERSION},
        "producer": {
            **producer,
            "implementation_family": "vAmiga",
            "binary_bytes": build["adapter"]["binary_bytes"],
            "binary_sha256": build["adapter"]["binary_sha256"],
        },
        "capture_adapter": build["adapter"]["source_sha256"],
        "producer_build": {
            "record_file": "producer-build-v1.json",
            "record_sha256": sha256(PACKAGE_ROOT / "producer-build-v1.json"),
            "log_file": "logs/producer-build.log",
            "log_sha256": sha256(PACKAGE_ROOT / "logs" / "producer-build.log"),
        },
        "matrix": {
            "case_count": len(CASES),
            "cases": list(CASES),
            "machine": "A500 OCS PAL",
            "sample_rate_hz": 48_000,
            "sample_format": "IEEE-754 binary32 little-endian WAVE",
            "capture_domain": "modelled-analogue-output",
            "captured_guest_fields": [9, 10, 11],
        },
        "packager": {
            "script": "package.py",
            "script_sha256": sha256(PACKAGE_ROOT / "package.py"),
            "python_version": sys.version.split()[0],
        },
        "runs": runs,
    }
    write_json(PACKAGE_ROOT / "package-v1.json", package)
    verify_package()


def verify_package() -> None:
    manifest = read_json(PACKAGE_ROOT / "package-v1.json")
    require(manifest.get("schema_version") == "1.0.0", "bad package schema")
    require(
        manifest.get("suite") == {"id": SUITE_ID, "version": SUITE_VERSION},
        "bad package suite identity",
    )
    runs = manifest.get("runs")
    require(
        isinstance(runs, list)
        and [run.get("case_id") for run in runs] == list(CASES),
        "bad package run matrix",
    )
    for run in runs:
        for file_key, hash_key in (
            ("capture_file", "capture_sha256"),
            ("configuration_file", "configuration_sha256"),
            ("manifest_file", "manifest_sha256"),
            ("producer_log_file", "producer_log_sha256"),
            ("record_file", "record_sha256"),
        ):
            path = PACKAGE_ROOT / run[file_key]
            require_file(path)
            require(
                sha256(path) == run[hash_key],
                f"{path}: package hash mismatch",
            )
        record = read_json(PACKAGE_ROOT / run["record_file"])
        require(
            record.get("source_capture", {}).get("file_sha256")
            == run["capture_sha256"],
            f"{run['case_id']}: record capture hash mismatch",
        )
        validate_observations(
            run["case_id"],
            record,
            PACKAGE_ROOT / run["capture_file"],
        )
    build = manifest.get("producer_build")
    require(isinstance(build, dict), "missing producer build identity")
    for file_key, hash_key in (
        ("record_file", "record_sha256"),
        ("log_file", "log_sha256"),
    ):
        path = PACKAGE_ROOT / build[file_key]
        require_file(path)
        require(sha256(path) == build[hash_key], f"{path}: hash mismatch")
    require(
        manifest.get("packager", {}).get("script_sha256")
        == sha256(PACKAGE_ROOT / "package.py"),
        "packager identity mismatch",
    )
    print(f"verified vAmiga Paula package: {len(runs)} cases")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    create = commands.add_parser("create")
    create.add_argument("capture_root", type=Path)
    commands.add_parser("verify")
    return result


def main() -> int:
    try:
        args = parser().parse_args()
        if args.command == "create":
            create_package(args.capture_root.resolve())
        else:
            verify_package()
        return 0
    except PackageError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
