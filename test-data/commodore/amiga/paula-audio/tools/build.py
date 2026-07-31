#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build and validate every Paula-audio corpus artifact."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from make_adf import ADF_BYTES, BOOTBLOCK_BYTES, pack_adf, validate_adf

ROOT = Path(__file__).resolve().parents[1]
CASE_FILE = ROOT / "cases" / "cases.json"
SOURCE_DIR = ROOT / "src"
SCHEMA_DIR = ROOT / "schema"
DEFAULT_OUTPUT = ROOT / "dist"
LOAD_ADDRESS = 0x00030000

CASE_ID_PATTERN = re.compile(r"^[a-z][a-z0-9-]*$")
SERIAL_PATTERN = re.compile(r"^[\x20-\x7e]+$")
WORD_PATTERN = re.compile(r"^0x[0-9a-f]{4}$")
COLOR_PATTERN = re.compile(r"^0x[0-9a-f]{3}$")

HASHED_SOURCES = (
    "src/bootblock.S",
    "src/probe.S",
    "src/custom-registers.inc",
    "tools/build.py",
    "tools/make_adf.py",
    "schema/suite-v1.schema.json",
    "schema/capture-v1.schema.json",
)


class BuildError(RuntimeError):
    """Raised when source inputs or build outputs violate the corpus contract."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def merge_case(defaults: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    merged = copy.deepcopy(defaults)
    for key, value in source.items():
        merged[key] = copy.deepcopy(value)
    return merged


def validate_case(case: dict[str, Any]) -> None:
    case_id = case.get("id")
    if not isinstance(case_id, str) or not CASE_ID_PATTERN.fullmatch(case_id):
        raise BuildError(f"invalid case id: {case_id!r}")
    if not isinstance(case.get("numeric_id"), int) or not 1 <= case["numeric_id"] <= 65535:
        raise BuildError(f"{case_id}.numeric_id must be a positive 16-bit integer")
    question = case.get("question")
    if not isinstance(question, str) or not question.endswith("?"):
        raise BuildError(f"{case_id}.question must contain one explicit question")

    channel = case.get("channel")
    if not isinstance(channel, int) or not 0 <= channel <= 3:
        raise BuildError(f"{case_id}.channel must be in the range 0..3")
    volume = case.get("volume")
    if not isinstance(volume, int) or not 0 <= volume <= 64:
        raise BuildError(f"{case_id}.volume must be in the range 0..64")
    period = case.get("period_cck")
    if not isinstance(period, int) or not 124 <= period <= 65535:
        raise BuildError(f"{case_id}.period_cck must be in the range 124..65535")

    sample = case.get("sample")
    if not isinstance(sample, dict) or set(sample) != {"word", "encoding", "words"}:
        raise BuildError(f"{case_id}.sample has the wrong fields")
    if not isinstance(sample["word"], str) or not WORD_PATTERN.fullmatch(sample["word"]):
        raise BuildError(f"{case_id}.sample.word must be a lowercase 16-bit word")
    if sample["encoding"] != "two signed 8-bit PCM samples, high byte first":
        raise BuildError(f"{case_id}.sample.encoding changed")
    if not isinstance(sample["words"], int) or not 1 <= sample["words"] <= 65535:
        raise BuildError(f"{case_id}.sample.words must be a positive 16-bit count")

    visual = case.get("visual_identity")
    if not isinstance(visual, dict) or set(visual) != {"color00", "description"}:
        raise BuildError(f"{case_id}.visual_identity has the wrong fields")
    color = visual["color00"]
    if not isinstance(color, str) or not COLOR_PATTERN.fullmatch(color) or color == "0x000":
        raise BuildError(f"{case_id}.visual_identity.color00 must be non-black RGB4")

    serial = case.get("serial_identity")
    if (
        not isinstance(serial, str)
        or not SERIAL_PATTERN.fullmatch(serial)
        or "\0" in serial
        or len(serial.encode("ascii")) + 1 > 64
    ):
        raise BuildError(f"{case_id}.serial_identity must fit 64 printable ASCII bytes")

    expected = case.get("expected")
    if expected != {"status": "unresolved", "observations": []}:
        raise BuildError(f"{case_id}.expected must remain unresolved and empty")

    capture = case.get("capture")
    if not isinstance(capture, dict):
        raise BuildError(f"{case_id}.capture is missing")
    if capture.get("ready_record_address") != "0x0002ff00":
        raise BuildError(f"{case_id}.capture ready address changed")
    if capture.get("ready_magic") != "PAUD":
        raise BuildError(f"{case_id}.capture magic changed")
    if capture.get("byte_order") != "big-endian":
        raise BuildError(f"{case_id}.capture byte order changed")
    if capture.get("settle_fields", 0) < 1 or capture.get("capture_fields", 0) < 3:
        raise BuildError(f"{case_id}.capture window is too short")
    if capture.get("ready_timeout_fields", 0) <= capture["settle_fields"]:
        raise BuildError(f"{case_id}.capture timeout must exceed settle time")
    if capture.get("automatic_gain_control") is not False:
        raise BuildError(f"{case_id}.capture must disable automatic gain")
    if capture.get("channel_remapping") is not False:
        raise BuildError(f"{case_id}.capture must retain channel order")

    applicability = case.get("applicability")
    if not isinstance(applicability, dict):
        raise BuildError(f"{case_id}.applicability is missing")
    if applicability.get("regions") != ["PAL"]:
        raise BuildError(f"{case_id} must retain the controlled PAL region")
    if applicability.get("min_chip_ram_bytes", 0) < 0x40000:
        raise BuildError(f"{case_id} needs chip RAM through the payload address")


def validate_comparisons(cases: list[dict[str, Any]]) -> None:
    by_id = {case["id"]: case for case in cases}
    for case in cases:
        comparison = case.get("comparison")
        if comparison is None:
            continue
        if not isinstance(comparison, dict) or set(comparison) != {
            "case_id",
            "differing_fields",
        }:
            raise BuildError(f"{case['id']}.comparison has the wrong fields")
        baseline_id = comparison["case_id"]
        if baseline_id not in by_id or baseline_id == case["id"]:
            raise BuildError(f"{case['id']} comparison baseline is invalid")
        differing = comparison["differing_fields"]
        if not isinstance(differing, list) or len(differing) != len(set(differing)):
            raise BuildError(f"{case['id']} differing_fields must be unique")

        baseline = by_id[baseline_id]
        actual = {
            key
            for key in set(case) | set(baseline)
            if case.get(key) != baseline.get(key)
        }
        if actual != set(differing):
            raise BuildError(
                f"{case['id']} differs from {baseline_id} in {sorted(actual)}, "
                f"not declared {sorted(differing)}"
            )


def load_cases() -> tuple[dict[str, Any], list[dict[str, Any]]]:
    source = json.loads(CASE_FILE.read_text(encoding="utf-8"))
    if source.get("source_format_version") != 1:
        raise BuildError("unsupported cases.json source_format_version")
    suite = source.get("suite")
    required_suite = {"id", "version", "license", "source_revision"}
    if not isinstance(suite, dict) or set(suite) != required_suite:
        raise BuildError("suite metadata has the wrong fields")
    if suite["id"] != "org.198x.amiga.paula-audio":
        raise BuildError("suite id changed")
    if suite["license"] != "CC0-1.0":
        raise BuildError("suite license must be CC0-1.0")

    defaults = source.get("defaults")
    source_cases = source.get("cases")
    if not isinstance(defaults, dict) or not isinstance(source_cases, list):
        raise BuildError("cases.json must contain defaults and a cases array")

    cases = [merge_case(defaults, case) for case in source_cases]
    for case in cases:
        validate_case(case)
    validate_comparisons(cases)

    ids = [case["id"] for case in cases]
    numeric_ids = [case["numeric_id"] for case in cases]
    if len(ids) != len(set(ids)) or len(numeric_ids) != len(set(numeric_ids)):
        raise BuildError("case ids and numeric ids must be unique")
    if numeric_ids != sorted(numeric_ids):
        raise BuildError("cases must be ordered by numeric_id")
    return suite, cases


def resolve_tool(command: str) -> Path:
    found = shutil.which(command)
    if found is None:
        raise BuildError(f"required tool not found on PATH: {command}")
    return Path(found).resolve()


def run(command: list[str], *, env: dict[str, str]) -> str:
    result = subprocess.run(
        command,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        rendered = " ".join(command)
        raise BuildError(f"command failed ({result.returncode}): {rendered}\n{result.stdout}")
    return result.stdout


def tool_version(tool: Path, env: dict[str, str]) -> str:
    output = run([str(tool), "--version"], env=env)
    first_line = output.splitlines()
    if not first_line:
        raise BuildError(f"{tool} returned an empty version")
    return first_line[0].strip()


def generated_case_include(case: dict[str, Any]) -> str:
    channel = case["channel"]
    base = 0x0A0 + channel * 0x10
    dma_word = 0x8000 | 0x0200 | (1 << channel)
    serial = case["serial_identity"]
    lines = [
        "/* SPDX-License-Identifier: CC0-1.0 */",
        "/* Generated from cases/cases.json; do not edit. */",
        f".equ CASE_NUMBER, {case['numeric_id']}",
        f".equ CASE_CHANNEL, {channel}",
        f".equ CASE_VOLUME, {case['volume']}",
        f".equ CASE_PERIOD_CCK, {case['period_cck']}",
        f".equ CASE_SAMPLE_WORD, {case['sample']['word']}",
        f".equ CASE_SAMPLE_WORDS, {case['sample']['words']}",
        f".equ CASE_COLOR00, {case['visual_identity']['color00']}",
        f".equ CASE_DMACON_WORD, 0x{dma_word:04x}",
        f".equ CASE_AUD_LCH, 0x{base:03x}",
        f".equ CASE_AUD_LCL, 0x{base + 2:03x}",
        f".equ CASE_AUD_LEN, 0x{base + 4:03x}",
        f".equ CASE_AUD_PER, 0x{base + 6:03x}",
        f".equ CASE_AUD_VOL, 0x{base + 8:03x}",
        "",
        ".macro EMIT_CASE_IDENTITY",
        f'    .ascii "{serial}"',
        "    .byte 0",
        ".endm",
        "",
    ]
    return "\n".join(lines)


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_bytes(data)
    os.replace(temporary, path)


def assemble_payload(
    case: dict[str, Any],
    case_dir: Path,
    assembler: Path,
    linker: Path,
    env: dict[str, str],
) -> bytes:
    include_path = case_dir / "case.inc"
    include_path.write_text(generated_case_include(case), encoding="ascii", newline="\n")
    object_path = case_dir / "probe.o"
    binary_path = case_dir / "payload.bin"

    run(
        [
            str(assembler),
            "-m68000",
            "--register-prefix-optional",
            "--fatal-warnings",
            "-I",
            str(SOURCE_DIR),
            "-I",
            str(case_dir),
            "-o",
            str(object_path),
            str(SOURCE_DIR / "probe.S"),
        ],
        env=env,
    )
    run(
        [
            str(linker),
            "-m",
            "m68kelf",
            "-Ttext",
            f"0x{LOAD_ADDRESS:x}",
            "-e",
            "_start",
            "--build-id=none",
            "--oformat",
            "binary",
            "-o",
            str(binary_path),
            str(object_path),
        ],
        env=env,
    )
    payload = binary_path.read_bytes()
    if not payload:
        raise BuildError(f"{case['id']} produced an empty payload")
    return payload


def assemble_bootblock(
    sectors: int,
    case_dir: Path,
    assembler: Path,
    linker: Path,
    env: dict[str, str],
) -> bytes:
    object_path = case_dir / "bootblock.o"
    binary_path = case_dir / "bootblock.bin"
    run(
        [
            str(assembler),
            "-m68000",
            "--register-prefix-optional",
            "--fatal-warnings",
            "--defsym",
            f"PAYLOAD_SECTORS={sectors}",
            "-o",
            str(object_path),
            str(SOURCE_DIR / "bootblock.S"),
        ],
        env=env,
    )
    run(
        [
            str(linker),
            "-m",
            "m68kelf",
            "-Ttext",
            "0",
            "-e",
            "_start",
            "--build-id=none",
            "--oformat",
            "binary",
            "-o",
            str(binary_path),
            str(object_path),
        ],
        env=env,
    )
    bootblock = binary_path.read_bytes()
    if len(bootblock) > BOOTBLOCK_BYTES:
        raise BuildError(f"boot block grew to {len(bootblock)} bytes")
    return bootblock


def source_hashes() -> dict[str, str]:
    hashes: dict[str, str] = {}
    for relative in HASHED_SOURCES:
        path = ROOT / relative
        if not path.is_file():
            raise BuildError(f"hashed source is missing: {relative}")
        hashes[relative] = sha256_file(path)
    return hashes


def validate_manifest(manifest: dict[str, Any], cases: list[dict[str, Any]]) -> None:
    required = {"schema_version", "suite", "build", "cases", "artifacts"}
    if set(manifest) != required:
        raise BuildError("generated suite manifest has the wrong top-level fields")
    if manifest["schema_version"] != "1.0.0":
        raise BuildError("generated suite manifest has the wrong schema version")
    if manifest["cases"] != cases:
        raise BuildError("generated suite manifest changed expanded case records")
    artifacts = manifest["artifacts"]
    if [entry["case_id"] for entry in artifacts] != [case["id"] for case in cases]:
        raise BuildError("generated artifacts are not in case order")
    if any(entry["adf_bytes"] != ADF_BYTES for entry in artifacts):
        raise BuildError("generated artifact has the wrong ADF size")

    for schema_name in ("suite-v1.schema.json", "capture-v1.schema.json"):
        json.loads((SCHEMA_DIR / schema_name).read_text(encoding="utf-8"))


def build(output: Path, assembler_name: str, linker_name: str) -> dict[str, Any]:
    suite, cases = load_cases()
    assembler = resolve_tool(assembler_name)
    linker = resolve_tool(linker_name)
    env = dict(os.environ)
    env.update({"LC_ALL": "C", "LANG": "C", "SOURCE_DATE_EPOCH": "0"})

    output.mkdir(parents=True, exist_ok=True)
    artifacts: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="paula-audio-build-") as temporary:
        temporary_root = Path(temporary)
        for case in cases:
            case_dir = temporary_root / case["id"]
            case_dir.mkdir()
            payload = assemble_payload(case, case_dir, assembler, linker, env)
            sectors = (len(payload) + 511) // 512
            bootblock = assemble_bootblock(sectors, case_dir, assembler, linker, env)
            adf, boot_checksum, packed_sectors = pack_adf(bootblock, payload)
            if packed_sectors != sectors:
                raise BuildError(f"{case['id']} payload sector count changed while packing")
            validate_adf(adf, payload, sectors)

            adf_name = f"{case['id']}.adf"
            payload_name = f"{case['id']}.bin"
            atomic_write(output / adf_name, adf)
            atomic_write(output / payload_name, payload)
            artifacts.append(
                {
                    "case_id": case["id"],
                    "adf_file": adf_name,
                    "payload_file": payload_name,
                    "adf_bytes": len(adf),
                    "payload_bytes": len(payload),
                    "sha256": {
                        "adf": sha256_bytes(adf),
                        "payload": sha256_bytes(payload),
                    },
                    "load_address": LOAD_ADDRESS,
                    "sectors": sectors,
                    "bootblock_checksum": f"0x{boot_checksum:08x}",
                }
            )

    manifest = {
        "schema_version": "1.0.0",
        "suite": suite,
        "build": {
            "toolchain": {
                "target": "m68k-elf",
                "assembler": {
                    "command": assembler.name,
                    "version": tool_version(assembler, env),
                },
                "linker": {
                    "command": linker.name,
                    "version": tool_version(linker, env),
                },
                "python": {
                    "implementation": platform.python_implementation(),
                    "version": platform.python_version(),
                },
            },
            "case_file_sha256": sha256_file(CASE_FILE),
            "source_sha256": source_hashes(),
        },
        "cases": cases,
        "artifacts": artifacts,
    }
    validate_manifest(manifest, cases)
    manifest_bytes = (
        json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=True) + "\n"
    ).encode("utf-8")
    atomic_write(output / "suite-v1.json", manifest_bytes)
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build the emulator-neutral Paula-audio corpus."
    )
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--assembler", default="m68k-elf-as")
    parser.add_argument("--linker", default="m68k-elf-ld")
    args = parser.parse_args()

    output = args.output.resolve()
    manifest = build(output, args.assembler, args.linker)
    for artifact in manifest["artifacts"]:
        print(
            f"{artifact['case_id']}: {artifact['adf_file']} "
            f"sha256={artifact['sha256']['adf']} "
            f"payload={artifact['payload_bytes']}B/{artifact['sectors']} sectors"
        )
    print(f"manifest: {output / 'suite-v1.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
