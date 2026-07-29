#!/usr/bin/env python3
"""Snapshot and identify every input to one Copperline capture run."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


PRODUCER_REVISION = "eec5806778dab8b60f3b05fa7ab2428e4e18b073"
PRODUCER_BINARY_SHA256 = (
    "ead4139d547085ad58a9794b17e57e6bf0649e4c6c7040e038f00550030a7fe9"
)
PRODUCER_VERSION_OUTPUT = "copperline 0.13.0"
SUITE_ID = "org.198x.amiga.programmable-hblank"
SUITE_VERSION = "1.0.1"
CAPTURE_ENVIRONMENT = {
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


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture_tool_hashes() -> dict[str, str]:
    tool_dir = Path(__file__).resolve().parent
    return {
        "capture.sh": sha256_file(tool_dir / "capture.sh"),
        "capture_manifest.py": sha256_file(tool_dir / "capture_manifest.py"),
    }


def producer_version(binary: Path) -> str:
    result = subprocess.run(
        [binary, "--version"],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def host_identity() -> str:
    if sys.platform == "darwin":
        version = subprocess.run(
            ["sw_vers", "-productVersion"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        build = subprocess.run(
            ["sw_vers", "-buildVersion"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        return f"macOS {version} build {build}, {platform.machine()}"
    return f"{platform.system()} {platform.release()}, {platform.machine()}"


def verified_capture_environment() -> dict[str, str]:
    actual = {
        name: value
        for name, value in os.environ.items()
        if name == "RUST_LOG" or name.startswith("COPPERLINE_")
    }
    undeclared = sorted(set(actual) - set(CAPTURE_ENVIRONMENT))
    if undeclared:
        raise ValueError(
            "undeclared producer environment variables are set: "
            + ", ".join(undeclared)
        )
    if actual != CAPTURE_ENVIRONMENT:
        mismatches = [
            f"{name}={actual.get(name)!r}, expected {value!r}"
            for name, value in CAPTURE_ENVIRONMENT.items()
            if actual.get(name) != value
        ]
        raise ValueError(
            "capture environment does not match the canonical values: "
            + "; ".join(mismatches)
        )
    return actual


def verify_producer(binary: Path) -> None:
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f"producer binary is not executable: {binary}")
    actual_sha256 = sha256_file(binary)
    if actual_sha256 != PRODUCER_BINARY_SHA256:
        raise ValueError(
            f"producer binary SHA-256 is {actual_sha256}; "
            f"expected {PRODUCER_BINARY_SHA256}"
        )
    actual_version = producer_version(binary)
    if actual_version != PRODUCER_VERSION_OUTPUT:
        raise ValueError(
            f"producer version is {actual_version!r}; "
            f"expected {PRODUCER_VERSION_OUTPUT!r}"
        )


def find_case(
    suite: dict[str, Any], case_id: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    cases = [case for case in suite["cases"] if case["id"] == case_id]
    artifacts = [
        artifact for artifact in suite["artifacts"] if artifact["case_id"] == case_id
    ]
    if len(cases) != 1 or len(artifacts) != 1:
        raise ValueError(f"suite does not contain one case and artifact for {case_id}")
    return cases[0], artifacts[0]


def copy_verified(source: Path, destination: Path, expected_sha256: str) -> None:
    actual_sha256 = sha256_file(source)
    if actual_sha256 != expected_sha256:
        raise ValueError(
            f"{source.name} SHA-256 is {actual_sha256}; expected {expected_sha256}"
        )
    shutil.copyfile(source, destination)
    if sha256_file(destination) != expected_sha256:
        raise ValueError(f"copied input changed: {destination}")


def prepare(args: argparse.Namespace) -> None:
    binary = args.copperline.resolve()
    suite_dir = args.suite_dir.resolve()
    firmware = args.firmware.resolve()
    output_dir = args.output_dir.resolve()
    if output_dir.exists():
        raise ValueError(f"output already exists: {output_dir}")
    if not args.operator.strip():
        raise ValueError("operator must not be empty")
    verify_producer(binary)
    capture_environment = verified_capture_environment()

    suite_path = suite_dir / "suite-v1.json"
    suite = json.loads(suite_path.read_text(encoding="utf-8"))
    if suite["suite"]["id"] != SUITE_ID or suite["suite"]["version"] != SUITE_VERSION:
        raise ValueError("suite identity does not match this capture package")
    case, artifact = find_case(suite, args.case_id)

    output_dir.mkdir(parents=True)
    (output_dir / "frames").mkdir()
    config_source = Path(__file__).resolve().parent / f"{args.profile}.toml"
    shutil.copyfile(config_source, output_dir / "capture.toml")
    shutil.copyfile(suite_path, output_dir / "suite-v1.json")
    shutil.copyfile(firmware, output_dir / "firmware.rom")
    copy_verified(
        suite_dir / artifact["adf_file"],
        output_dir / "stimulus.adf",
        artifact["sha256"]["adf"],
    )
    copy_verified(
        suite_dir / artifact["payload_file"],
        output_dir / "stimulus.bin",
        artifact["sha256"]["payload"],
    )

    inputs = {
        name: sha256_file(output_dir / name)
        for name in [
            "capture.toml",
            "firmware.rom",
            "stimulus.adf",
            "stimulus.bin",
            "suite-v1.json",
        ]
    }
    manifest = {
        "schema_version": "1.0.0",
        "capture_tools": capture_tool_hashes(),
        "producer": {
            "product": "Copperline",
            "version": "0.13.0",
            "revision": PRODUCER_REVISION,
            "source_url": "https://github.com/CopperlineHQ/Copperline",
            "binary_sha256": PRODUCER_BINARY_SHA256,
            "version_output": PRODUCER_VERSION_OUTPUT,
        },
        "capture": {
            "captured_at_utc": datetime.datetime.now(
                datetime.timezone.utc
            ).isoformat(timespec="seconds"),
            "operator": args.operator,
            "host": host_identity(),
            "profile": args.profile,
            "case_id": args.case_id,
            "environment": capture_environment,
            "command": [
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
            ],
        },
        "suite": {
            "id": suite["suite"]["id"],
            "version": suite["suite"]["version"],
            "source_revision": suite["suite"]["source_revision"],
            "case_id": case["id"],
            "numeric_id": case["numeric_id"],
            "adf_file": artifact["adf_file"],
            "adf_sha256": artifact["sha256"]["adf"],
            "payload_file": artifact["payload_file"],
            "payload_sha256": artifact["sha256"]["payload"],
        },
        "inputs": inputs,
    }
    (output_dir / "capture-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def verify(args: argparse.Namespace) -> None:
    binary = args.copperline.resolve()
    output_dir = args.output_dir.resolve()
    verify_producer(binary)
    capture_environment = verified_capture_environment()
    manifest = json.loads(
        (output_dir / "capture-manifest.json").read_text(encoding="utf-8")
    )
    if manifest["producer"]["binary_sha256"] != PRODUCER_BINARY_SHA256:
        raise ValueError("capture manifest names another producer binary")
    if manifest["capture_tools"] != capture_tool_hashes():
        raise ValueError("capture tools changed during execution")
    if manifest["capture"]["environment"] != capture_environment:
        raise ValueError("capture environment changed during execution")
    for name, expected_sha256 in manifest["inputs"].items():
        path = output_dir / name
        if sha256_file(path) != expected_sha256:
            raise ValueError(f"captured input changed during execution: {path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("profile", choices=["ecs", "aga"])
    prepare_parser.add_argument("case_id")
    prepare_parser.add_argument("copperline", type=Path)
    prepare_parser.add_argument("suite_dir", type=Path)
    prepare_parser.add_argument("firmware", type=Path)
    prepare_parser.add_argument("output_dir", type=Path)
    prepare_parser.add_argument("operator")

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("copperline", type=Path)
    verify_parser.add_argument("output_dir", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "prepare":
            prepare(args)
        else:
            verify(args)
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
