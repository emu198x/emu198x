#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build and validate the OCS Copper colour output-phase corpus."""

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
    "bpl1mod",
    "bpl2mod",
    "dmacon_enable",
)
MOVE_COUNT = 4

HASHED_SOURCES = (
    "src/bootblock.S",
    "src/probe.S",
    "src/custom-registers.inc",
    "tools/build.py",
    "tools/make_adf.py",
    "tools/validate_capture.py",
    "tools/test_build.py",
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
        "color_program",
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
    if case_id != "adjacent-color00-moves" or case.get("numeric_id") != 1:
        raise BuildError("source version 1 contains only adjacent-color00-moves case 1")
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
    if applicability["chipsets"] != ["OCS"]:
        raise BuildError(f"{case_id} must remain an OCS-only probe")
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
    if any(words[name] != 0 for name in ("bplcon1", "bplcon2")):
        raise BuildError(f"{case_id} must retain zeroed common bitplane controls")
    if any(words[name] != 0 for name in ("bpl1mod", "bpl2mod")):
        raise BuildError(f"{case_id} bitplane rows must remain contiguous")
    if words["dmacon_enable"] != 0x8380:
        raise BuildError(
            f"{case_id} must enable only master, bitplane, and Copper DMA"
        )

    color_program = case.get("color_program")
    color_fields = {
        "target_register",
        "guard_word",
        "marker_register",
        "marker_word",
        "move_words",
        "move_order",
    }
    if not isinstance(color_program, dict) or set(color_program) != color_fields:
        raise BuildError(f"{case_id}.color_program has the wrong fields")
    if color_program["target_register"] != "COLOR00":
        raise BuildError(f"{case_id} must target COLOR00")
    if color_program["marker_register"] != "COLOR01":
        raise BuildError(f"{case_id} marker must use COLOR01")
    guard = parse_word(color_program["guard_word"], f"{case_id}.guard_word")
    marker = parse_word(color_program["marker_word"], f"{case_id}.marker_word")
    move_values = color_program.get("move_words")
    if not isinstance(move_values, list) or len(move_values) != MOVE_COUNT:
        raise BuildError(f"{case_id} must contain exactly four COLOR00 MOVE words")
    moves = [
        parse_word(value, f"{case_id}.move_words[{index}]")
        for index, value in enumerate(move_values)
    ]
    if any(value > 0x0FFF for value in [guard, marker, *moves]):
        raise BuildError(f"{case_id} colours must be OCS RGB4 words")
    if len(set([guard, marker, *moves])) != MOVE_COUNT + 2:
        raise BuildError(f"{case_id} visual identity colours must be distinct")
    if color_program.get("move_order") != [
        "guard-to-red",
        "red-to-green",
        "green-to-blue",
        "blue-to-yellow",
    ]:
        raise BuildError(f"{case_id}.move_order changed")

    geometry = case.get("geometry")
    geometry_fields = {
        "resolution",
        "sample_line_first",
        "sample_line_last_exclusive",
        "sample_beam_line",
        "reset_wait_hpos_cck",
        "move_wait_hpos_cck",
        "bitplane_rows",
        "bitplane_words_per_row",
        "marker_word_index",
        "marker_bit_index",
        "horizontal_mapping",
        "anchor",
    }
    if not isinstance(geometry, dict) or set(geometry) != geometry_fields:
        raise BuildError(f"{case_id}.geometry has the wrong fields")
    if geometry["resolution"] != "lores":
        raise BuildError(f"{case_id} geometry must remain low resolution")
    if geometry["horizontal_mapping"] != "producer-recorded":
        raise BuildError(f"{case_id} may not embed an expected horizontal mapping")
    if geometry["anchor"] != (
        "DMA-fed one-bitplane marker within a fixed display window"
    ):
        raise BuildError(f"{case_id} anchor definition changed")

    first_line = geometry["sample_line_first"]
    last_line = geometry["sample_line_last_exclusive"]
    sample_line = geometry["sample_beam_line"]
    if not all(isinstance(value, int) for value in (first_line, last_line, sample_line)):
        raise BuildError(f"{case_id} sample lines must be integers")
    if (first_line, last_line) != (128, 136):
        raise BuildError(f"{case_id} must retain its eight-line sample band")
    if not first_line <= sample_line < last_line:
        raise BuildError(f"{case_id} measurement line is outside the sample band")

    reset_hpos = geometry["reset_wait_hpos_cck"]
    move_hpos = geometry["move_wait_hpos_cck"]
    for name, hpos in (("reset_wait_hpos_cck", reset_hpos), ("move_wait_hpos_cck", move_hpos)):
        if not isinstance(hpos, int) or not 0 <= hpos <= 226 or hpos & 1:
            raise BuildError(f"{case_id}.{name} must be an even PAL CCK")
    if not reset_hpos < words["ddfstrt"] < words["ddfstop"] < move_hpos:
        raise BuildError(
            f"{case_id} colour MOVEs must begin after the bitplane DMA fetch ends"
        )

    ddf_span = words["ddfstop"] - words["ddfstrt"]
    if ddf_span < 0 or ddf_span % 8 != 0:
        raise BuildError(f"{case_id} DDF interval is not an integral lowres fetch span")
    if geometry["bitplane_words_per_row"] != ddf_span // 8 + 1:
        raise BuildError(f"{case_id} row width does not match DDFSTRT/DDFSTOP")
    if geometry["bitplane_rows"] < 256:
        raise BuildError(f"{case_id} bitplane does not cover the display window")
    if not 0 <= geometry["marker_word_index"] < geometry["bitplane_words_per_row"]:
        raise BuildError(f"{case_id} marker word is outside the row")
    if not 0 <= geometry["marker_bit_index"] <= 15:
        raise BuildError(f"{case_id} marker bit is outside its word")

    schedule = case.get("programming_schedule")
    if not isinstance(schedule, dict) or set(schedule) != {
        "phase",
        "write_order",
        "move_spacing",
        "steady_state",
    }:
        raise BuildError(f"{case_id}.programming_schedule has the wrong fields")
    if schedule["write_order"] != [
        "early-line COLOR00 guard restore",
        "fixed horizontal WAIT",
        "COLOR00 red MOVE",
        "COLOR00 green MOVE",
        "COLOR00 blue MOVE",
        "COLOR00 yellow MOVE",
    ]:
        raise BuildError(f"{case_id}.programming_schedule.write_order changed")
    if schedule["move_spacing"] != (
        "four back-to-back Copper MOVE instructions with no intervening WAIT"
    ):
        raise BuildError(f"{case_id} must retain adjacent Copper MOVEs")
    if not all(isinstance(schedule[key], str) and schedule[key] for key in ("phase", "move_spacing", "steady_state")):
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
        "ready_magic": "CCPH",
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
        "sequence",
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
    if identity["serial_address"] != "0x0002ff50":
        raise BuildError(f"{case_id}.identity serial address changed")
    if identity["serial_maximum_bytes"] != 64:
        raise BuildError(f"{case_id}.identity serial bound changed")
    if not isinstance(identity["sequence"], list) or len(identity["sequence"]) != MOVE_COUNT:
        raise BuildError(f"{case_id}.identity.sequence must name four transitions")

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
    if suite["id"] != "org.198x.amiga.copper-color-output-phase":
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
    colors = case["color_program"]
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
            f".equ CASE_GUARD_COLOR00, {colors['guard_word']}",
            f".equ CASE_MARKER_COLOR01, {colors['marker_word']}",
        ]
    )
    for index, word in enumerate(colors["move_words"]):
        lines.append(f".equ CASE_MOVE_COLOR00_{index}, {word}")
    lines.extend(
        [
            f".equ CASE_MOVE_COUNT, {MOVE_COUNT}",
            f".equ CASE_SAMPLE_LINE_FIRST, {geometry['sample_line_first']}",
            f".equ CASE_SAMPLE_LINE_LAST_EXCLUSIVE, {geometry['sample_line_last_exclusive']}",
            ".equ CASE_SAMPLE_LINE_COUNT, "
            "CASE_SAMPLE_LINE_LAST_EXCLUSIVE - CASE_SAMPLE_LINE_FIRST",
            f".equ CASE_SAMPLE_BEAM_LINE, {geometry['sample_beam_line']}",
            f".equ CASE_RESET_WAIT_HPOS_CCK, {geometry['reset_wait_hpos_cck']}",
            f".equ CASE_MOVE_WAIT_HPOS_CCK, {geometry['move_wait_hpos_cck']}",
            f".equ CASE_BITPLANE_ROWS, {geometry['bitplane_rows']}",
            f".equ CASE_BITPLANE_WORDS_PER_ROW, {geometry['bitplane_words_per_row']}",
            f".equ CASE_MARKER_WORD_INDEX, {geometry['marker_word_index']}",
            f".equ CASE_MARKER_BIT_INDEX, {geometry['marker_bit_index']}",
            ".equ CASE_MARKER_TRAILING_WORDS, "
            f"{geometry['bitplane_words_per_row'] - geometry['marker_word_index'] - 1}",
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
    with tempfile.TemporaryDirectory(prefix="copper-color-phase-build-") as temporary:
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
        description="Build the emulator-neutral OCS Copper colour phase corpus."
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
