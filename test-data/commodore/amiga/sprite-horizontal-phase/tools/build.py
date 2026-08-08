#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build and validate the sprite horizontal-phase corpus artifact."""

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

REGISTER_NAMES = (
    "diwstrt",
    "diwstop",
    "ddfstrt",
    "ddfstop",
    "bplcon0",
    "bplcon1",
    "bplcon2",
    "bplcon3",
    "bplcon4",
    "fmode",
    "bpl1mod",
    "bpl2mod",
    "spr0pos",
    "spr0ctl",
    "spr0data",
    "spr0datb",
    "color00",
    "color01",
    "color17",
    "dmacon_enable",
)

HASHED_SOURCES = (
    "src/bootblock.S",
    "src/probe.S",
    "src/custom-registers.inc",
    "tools/build.py",
    "tools/make_adf.py",
    "tools/validate_capture.py",
    "tools/test_validate_capture.py",
    "schema/suite-v1.schema.json",
    "schema/capture-v1.schema.json",
)


class BuildError(RuntimeError):
    """Raised when source inputs or build outputs violate the corpus contract."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def parse_word(value: Any, context: str) -> int:
    if not isinstance(value, str) or not WORD_PATTERN.fullmatch(value):
        raise BuildError(f"{context} must be a lowercase four-digit hexadecimal word")
    return int(value, 16)


def merge_case(defaults: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    merged = copy.deepcopy(defaults)
    for key, value in source.items():
        merged[key] = copy.deepcopy(value)
    return merged


def validate_case(case: dict[str, Any]) -> None:
    expected_fields = {
        "id",
        "numeric_id",
        "question",
        "applicability",
        "registers",
        "geometry",
        "programming_schedule",
        "capture",
        "identity",
        "expected",
    }
    if set(case) != expected_fields:
        raise BuildError("expanded case record has the wrong fields")

    case_id = case.get("id")
    if not isinstance(case_id, str) or not CASE_ID_PATTERN.fullmatch(case_id):
        raise BuildError(f"invalid case id: {case_id!r}")
    if case_id != "fixed-lores-sprite" or case.get("numeric_id") != 1:
        raise BuildError("source version 1 contains only fixed-lores-sprite case 1")
    question = case.get("question")
    if not isinstance(question, str) or not question.endswith("?"):
        raise BuildError(f"{case_id}.question must contain one explicit question")

    applicability = case.get("applicability")
    if not isinstance(applicability, dict) or set(applicability) != {
        "chipsets",
        "regions",
        "min_chip_ram_bytes",
    }:
        raise BuildError(f"{case_id}.applicability has the wrong fields")
    if applicability["chipsets"] != ["OCS", "ECS", "AGA"]:
        raise BuildError(f"{case_id} must retain the OCS/ECS/AGA profile order")
    if applicability["regions"] != ["PAL"]:
        raise BuildError(f"{case_id} must retain the controlled PAL region")
    if applicability["min_chip_ram_bytes"] < 0x80000:
        raise BuildError(f"{case_id} must require at least 512 KiB of chip RAM")

    registers = case.get("registers")
    if not isinstance(registers, dict) or tuple(registers) != REGISTER_NAMES:
        raise BuildError(f"{case_id}.registers has the wrong register order or set")
    words = {
        name: parse_word(registers[name], f"{case_id}.registers.{name}")
        for name in REGISTER_NAMES
    }

    if words["bplcon0"] != 0x1000:
        raise BuildError(f"{case_id} must select one low-resolution bitplane")
    if any(words[name] != 0 for name in ("bplcon1", "bplcon2", "bplcon3")):
        raise BuildError(f"{case_id} must retain zeroed common bitplane controls")
    if words["bplcon4"] != 0x0011:
        raise BuildError(f"{case_id} must select the OCS-compatible sprite palette")
    if words["fmode"] != 0:
        raise BuildError(f"{case_id} must retain 16-bit bitplane and sprite fetches")
    if words["bpl1mod"] != 0 or words["bpl2mod"] != 0:
        raise BuildError(f"{case_id} bitplane rows must remain contiguous")
    if words["dmacon_enable"] != 0x8320:
        raise BuildError(f"{case_id} must enable only master, bitplane, and sprite DMA")

    vstart = ((words["spr0pos"] >> 8) & 0xFF) | ((words["spr0ctl"] & 0x0004) << 6)
    vstop = ((words["spr0ctl"] >> 8) & 0xFF) | ((words["spr0ctl"] & 0x0002) << 7)
    hstart = ((words["spr0pos"] & 0x00FF) << 1) | (words["spr0ctl"] & 0x0001)
    if (vstart, vstop, hstart) != (128, 144, 200):
        raise BuildError(f"{case_id} fixed sprite control words changed")
    if words["spr0ctl"] & 0x0080:
        raise BuildError(f"{case_id} must keep sprite 0 unattached")
    if words["spr0data"] != 0xFFFF or words["spr0datb"] != 0:
        raise BuildError(f"{case_id} must retain a solid sprite colour index")

    geometry = case.get("geometry")
    geometry_fields = {
        "resolution",
        "sample_beam_line",
        "bitplane_rows",
        "bitplane_words_per_row",
        "marker_word_index",
        "marker_bit_index",
        "sprite_data_lines",
        "horizontal_mapping",
        "anchors",
    }
    if not isinstance(geometry, dict) or set(geometry) != geometry_fields:
        raise BuildError(f"{case_id}.geometry has the wrong fields")
    if geometry["resolution"] != "lores":
        raise BuildError(f"{case_id} geometry must remain low resolution")
    if geometry["horizontal_mapping"] != "producer-recorded":
        raise BuildError(f"{case_id} may not embed an expected horizontal mapping")
    if geometry["anchors"] != [
        "retained hardwired HBLANK",
        "DMA-fed one-bitplane marker",
    ]:
        raise BuildError(f"{case_id} anchor definitions changed")
    if geometry["sprite_data_lines"] != vstop - vstart:
        raise BuildError(f"{case_id} sprite data length does not match VSTART/VSTOP")
    if not vstart <= geometry["sample_beam_line"] < vstop:
        raise BuildError(f"{case_id} sample line must pass through the sprite")
    if geometry["bitplane_rows"] < 256:
        raise BuildError(f"{case_id} bitplane does not cover the display window")

    ddf_span = words["ddfstop"] - words["ddfstrt"]
    if ddf_span < 0 or ddf_span % 8 != 0:
        raise BuildError(f"{case_id} DDF interval is not an integral lowres fetch span")
    if geometry["bitplane_words_per_row"] != ddf_span // 8 + 1:
        raise BuildError(f"{case_id} row width does not match DDFSTRT/DDFSTOP")
    if not 0 <= geometry["marker_word_index"] < geometry["bitplane_words_per_row"]:
        raise BuildError(f"{case_id} marker word is outside the row")
    if not 0 <= geometry["marker_bit_index"] <= 15:
        raise BuildError(f"{case_id} marker bit is outside its word")

    colors = [words[name] for name in ("color00", "color01", "color17")]
    if any(color > 0x0FFF for color in colors) or len(set(colors)) != 3:
        raise BuildError(f"{case_id} identity colours must be distinct RGB4 words")

    schedule = case.get("programming_schedule")
    if not isinstance(schedule, dict) or set(schedule) != {"phase", "steady_state"}:
        raise BuildError(f"{case_id}.programming_schedule has the wrong fields")
    if not all(isinstance(schedule[key], str) and schedule[key] for key in schedule):
        raise BuildError(f"{case_id}.programming_schedule must be explicit")

    capture = case.get("capture")
    capture_fields = {
        "ready_record_address",
        "ready_magic",
        "case_number_address",
        "schema_version_address",
        "field_counter_address",
        "byte_order",
        "ready_timeout_fields",
        "settle_fields",
        "capture_fields",
        "adjacent_field_stability_required",
        "blanking_retained",
        "overscan_retained",
        "alignment_search",
    }
    if not isinstance(capture, dict) or set(capture) != capture_fields:
        raise BuildError(f"{case_id}.capture has the wrong fields")
    fixed_capture = {
        "ready_record_address": "0x0002ff00",
        "ready_magic": "SPHX",
        "case_number_address": "0x0002ff04",
        "schema_version_address": "0x0002ff06",
        "field_counter_address": "0x0002ff08",
        "byte_order": "big-endian",
        "adjacent_field_stability_required": True,
        "blanking_retained": True,
        "overscan_retained": True,
        "alignment_search": False,
    }
    for key, value in fixed_capture.items():
        if capture[key] != value:
            raise BuildError(f"{case_id}.capture.{key} changed")
    if capture["settle_fields"] < 8 or capture["capture_fields"] < 3:
        raise BuildError(f"{case_id}.capture window is too short")
    if capture["ready_timeout_fields"] <= capture["settle_fields"]:
        raise BuildError(f"{case_id}.capture timeout must exceed settle time")

    identity = case.get("identity")
    if not isinstance(identity, dict) or set(identity) != {
        "serial",
        "serial_address",
        "serial_maximum_bytes",
        "background",
        "marker",
        "sprite",
    }:
        raise BuildError(f"{case_id}.identity has the wrong fields")
    serial = identity["serial"]
    if (
        not isinstance(serial, str)
        or not SERIAL_PATTERN.fullmatch(serial)
        or "\0" in serial
        or len(serial.encode("ascii")) + 1 > identity["serial_maximum_bytes"]
    ):
        raise BuildError(f"{case_id}.identity.serial must fit printable ASCII")
    if identity["serial_address"] != "0x0002ff40":
        raise BuildError(f"{case_id}.identity serial address changed")
    if identity["serial_maximum_bytes"] != 64:
        raise BuildError(f"{case_id}.identity serial bound changed")

    if case.get("expected") != {"status": "unresolved", "observations": []}:
        raise BuildError(f"{case_id}.expected must remain unresolved and empty")


def load_cases() -> tuple[dict[str, Any], list[dict[str, Any]]]:
    source = json.loads(CASE_FILE.read_text(encoding="utf-8"))
    if set(source) != {"source_format_version", "suite", "defaults", "cases"}:
        raise BuildError("cases.json has the wrong top-level fields")
    if source["source_format_version"] != 1:
        raise BuildError("unsupported cases.json source_format_version")

    suite = source.get("suite")
    if not isinstance(suite, dict) or set(suite) != {
        "id",
        "version",
        "license",
        "source_revision",
    }:
        raise BuildError("suite metadata has the wrong fields")
    if suite["id"] != "org.198x.amiga.sprite-horizontal-phase":
        raise BuildError("suite id changed")
    if suite["license"] != "CC0-1.0":
        raise BuildError("suite license must be CC0-1.0")

    defaults = source.get("defaults")
    source_cases = source.get("cases")
    if not isinstance(defaults, dict) or not isinstance(source_cases, list):
        raise BuildError("cases.json must contain defaults and a cases array")
    if len(source_cases) != 1:
        raise BuildError("source version 1 must contain exactly one case")

    cases = [merge_case(defaults, case) for case in source_cases]
    for case in cases:
        validate_case(case)
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
    registers = case["registers"]
    geometry = case["geometry"]
    lines = [
        "/* SPDX-License-Identifier: CC0-1.0 */",
        "/* Generated from cases/cases.json; do not edit. */",
        f".equ CASE_NUMBER, {case['numeric_id']}",
    ]
    for register in REGISTER_NAMES:
        lines.append(f".equ CASE_{register.upper()}, {registers[register]}")
    lines.extend(
        [
            f".equ CASE_SAMPLE_BEAM_LINE, {geometry['sample_beam_line']}",
            f".equ CASE_BITPLANE_ROWS, {geometry['bitplane_rows']}",
            f".equ CASE_BITPLANE_WORDS_PER_ROW, {geometry['bitplane_words_per_row']}",
            f".equ CASE_MARKER_WORD_INDEX, {geometry['marker_word_index']}",
            f".equ CASE_MARKER_BIT_INDEX, {geometry['marker_bit_index']}",
            ".equ CASE_MARKER_TRAILING_WORDS, "
            f"{geometry['bitplane_words_per_row'] - geometry['marker_word_index'] - 1}",
            f".equ CASE_SPRITE_DATA_LINES, {geometry['sprite_data_lines']}",
            "",
            ".macro EMIT_CASE_IDENTITY",
            f'    .ascii "{case["identity"]["serial"]}"',
            "    .byte 0",
            ".endm",
            "",
        ]
    )
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
    if LOAD_ADDRESS + len(payload) > case["applicability"]["min_chip_ram_bytes"]:
        raise BuildError(f"{case['id']} payload exceeds its minimum chip-RAM profile")
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
    if set(manifest) != {"schema_version", "suite", "build", "cases", "artifacts"}:
        raise BuildError("generated suite manifest has the wrong top-level fields")
    if manifest["schema_version"] != "1.0.0":
        raise BuildError("generated suite manifest has the wrong schema version")
    if manifest["cases"] != cases:
        raise BuildError("generated suite manifest changed expanded case records")
    if len(manifest["artifacts"]) != 1:
        raise BuildError("generated suite manifest must contain one artifact")
    if manifest["artifacts"][0]["adf_bytes"] != ADF_BYTES:
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
    with tempfile.TemporaryDirectory(prefix="sprite-phase-build-") as temporary:
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
        description="Build the emulator-neutral sprite horizontal-phase corpus."
    )
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--assembler", default="m68k-elf-as")
    parser.add_argument("--linker", default="m68k-elf-ld")
    args = parser.parse_args()

    manifest = build(args.output.resolve(), args.assembler, args.linker)
    for artifact in manifest["artifacts"]:
        print(
            f"{artifact['case_id']}: {artifact['adf_file']} "
            f"sha256={artifact['sha256']['adf']} "
            f"payload={artifact['payload_bytes']}B/{artifact['sectors']} sector"
            f"{'' if artifact['sectors'] == 1 else 's'}"
        )
    print(f"manifest: {(args.output.resolve() / 'suite-v1.json')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
