#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build and validate every programmable-HBLANK write-timing artifact."""

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
SUITE_ID = "org.198x.amiga.programmable-hblank-write-timing"
SUITE_VERSION = "1.0.0"
SOURCE_REVISION = "source-v1"
CASE_IDS = (
    "midline-hbstrt-past",
    "midline-hbstop-future",
    "midline-ecsena-enable",
    "midline-extblken-enable",
    "midline-blanken-enable",
)

CASE_ID_PATTERN = re.compile(r"^[a-z][a-z0-9-]*$")
SERIAL_PATTERN = re.compile(r"^[\x20-\x7e]+$")

REGISTER_SYMBOLS: dict[str, dict[str, int]] = {
    "bplcon0": {"HIRES": 0x8000, "SHRES": 0x0040, "ECSENA": 0x0001},
    "bplcon3": {"EXTBLKEN": 0x0001},
    "beamcon0": {"PAL": 0x0020, "BLANKEN": 0x0008},
    "hbstrt": {},
    "hbstop": {},
}

TIMED_REGISTERS = {
    "BPLCON0": "bplcon0",
    "BPLCON3": "bplcon3",
    "BEAMCON0": "beamcon0",
    "HBSTRT": "hbstrt",
    "HBSTOP": "hbstop",
}

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


def parse_word(value: Any, context: str) -> int:
    if not isinstance(value, str) or not re.fullmatch(r"0x[0-9a-f]{4}", value):
        raise BuildError(f"{context} must be a lowercase four-digit hexadecimal word")
    parsed = int(value, 16)
    if not 0 <= parsed <= 0xFFFF:
        raise BuildError(f"{context} is outside the 16-bit range")
    return parsed


def merge_case(defaults: dict[str, Any], source: dict[str, Any]) -> dict[str, Any]:
    merged = copy.deepcopy(defaults)
    for key, value in source.items():
        merged[key] = copy.deepcopy(value)

    color_word = merged["identity"]["visual"]["color00"]
    if not isinstance(color_word, str) or not re.fullmatch(
        r"0x[0-9a-f]{3}", color_word
    ):
        raise BuildError(f"{source['id']}.color00 must be a three-digit RGB word")
    color = int(color_word, 16)
    if color == 0:
        raise BuildError(f"{source['id']}.color00 must be non-zero")
    marker_word = merged["identity"]["visual"].get("marker_color00")
    if not isinstance(marker_word, str) or not re.fullmatch(
        r"0x[0-9a-f]{3}", marker_word
    ):
        raise BuildError(f"{source['id']}.marker_color00 must be a three-digit RGB word")
    marker = int(marker_word, 16)
    if marker == 0 or marker == color:
        raise BuildError(
            f"{source['id']}.marker_color00 must be non-zero and differ from color00"
        )
    merged["line_geometry"]["guard_color_word"] = f"0x{color:03x}"
    return merged


def validate_register(
    case_id: str, register_name: str, record: dict[str, Any]
) -> int:
    if set(record) != {"word", "symbols"}:
        raise BuildError(
            f"{case_id}.{register_name} must contain exactly word and symbols"
        )
    word = parse_word(record["word"], f"{case_id}.{register_name}.word")
    symbols = record["symbols"]
    if not isinstance(symbols, list) or len(symbols) != len(set(symbols)):
        raise BuildError(f"{case_id}.{register_name}.symbols must be unique")

    known = REGISTER_SYMBOLS[register_name]
    unknown = set(symbols) - set(known)
    if unknown:
        raise BuildError(
            f"{case_id}.{register_name} has unknown symbols: {sorted(unknown)}"
        )
    if not known:
        if symbols:
            raise BuildError(f"{case_id}.{register_name} does not define symbolic flags")
        return word

    symbolic_word = 0
    for symbol in symbols:
        symbolic_word |= known[symbol]
    if symbolic_word != word:
        raise BuildError(
            f"{case_id}.{register_name} word 0x{word:04x} does not match "
            f"declared symbols (0x{symbolic_word:04x})"
        )
    return word


def validate_case(case: dict[str, Any]) -> None:
    case_id = case.get("id")
    if not isinstance(case_id, str) or not CASE_ID_PATTERN.fullmatch(case_id):
        raise BuildError(f"invalid case id: {case_id!r}")
    if not isinstance(case.get("numeric_id"), int) or not 1 <= case["numeric_id"] <= 65535:
        raise BuildError(f"{case_id}.numeric_id must be a positive 16-bit integer")
    question = case.get("question")
    if not isinstance(question, str) or not question.endswith("?"):
        raise BuildError(f"{case_id}.question must contain one explicit question")

    registers = case.get("registers")
    if not isinstance(registers, dict) or set(registers) != set(REGISTER_SYMBOLS):
        raise BuildError(f"{case_id}.registers has the wrong register set")
    words = {
        name: validate_register(case_id, name, registers[name])
        for name in REGISTER_SYMBOLS
    }
    if words["hbstrt"] > 0x07FF or words["hbstop"] > 0x07FF:
        raise BuildError(f"{case_id} horizontal comparator words must be 11-bit")
    if "PAL" not in registers["beamcon0"]["symbols"]:
        raise BuildError(f"{case_id} must retain the BEAMCON0 PAL baseline")

    timed_write = case.get("timed_write")
    required_timed_write = {
        "reset_beam_line",
        "reset_hpos_cck",
        "beam_line",
        "wait_hpos_cck",
        "register",
        "word",
    }
    if not isinstance(timed_write, dict) or set(timed_write) != required_timed_write:
        raise BuildError(f"{case_id}.timed_write has the wrong fields")
    register = timed_write["register"]
    if register not in TIMED_REGISTERS:
        raise BuildError(f"{case_id}.timed_write.register is unsupported")
    if timed_write["reset_beam_line"] != 127 or timed_write["beam_line"] != 128:
        raise BuildError(f"{case_id}.timed_write must reset on line 127 and mutate on line 128")
    for name in ("reset_hpos_cck", "wait_hpos_cck"):
        hpos = timed_write[name]
        if not isinstance(hpos, int) or not 0 <= hpos <= 226 or hpos & 1:
            raise BuildError(f"{case_id}.timed_write.{name} must be an even PAL CCK")
    timed_word = parse_word(timed_write["word"], f"{case_id}.timed_write.word")
    register_name = TIMED_REGISTERS[register]
    if register in {"HBSTRT", "HBSTOP"} and timed_word > 0x07FF:
        raise BuildError(f"{case_id}.timed_write.word must be an 11-bit comparator")

    resolution = case.get("resolution")
    resolution_bits = words["bplcon0"] & 0x8040
    expected_bits = {"lores": 0x0000, "hires": 0x8000, "super-hires": 0x0040}
    if resolution not in expected_bits or resolution_bits != expected_bits[resolution]:
        raise BuildError(f"{case_id} BPLCON0 does not match resolution {resolution!r}")

    expected = case.get("expected")
    if expected != {"status": "unresolved", "observations": []}:
        raise BuildError(f"{case_id}.expected must remain unresolved and empty")

    identity = case.get("identity", {})
    visual = identity.get("visual", {})
    if visual.get("method") != "scheduled COLOR00 marker":
        raise BuildError(f"{case_id} must use the scheduled COLOR00 marker")
    serial = identity.get("serial", {})
    serial_value = serial.get("value")
    if (
        not isinstance(serial_value, str)
        or not SERIAL_PATTERN.fullmatch(serial_value)
        or "\0" in serial_value
    ):
        raise BuildError(f"{case_id} serial identity must be printable US-ASCII")
    encoded_length = len(serial_value.encode("ascii")) + 1
    maximum_bytes = serial.get("maximum_bytes")
    if maximum_bytes != 64 or encoded_length > maximum_bytes:
        raise BuildError(f"{case_id} serial identity does not fit its 64-byte record")

    settle = case.get("settle_capture", {})
    if settle.get("byte_order") != "big-endian":
        raise BuildError(f"{case_id} ready record must be big-endian")
    if settle.get("settle_fields", 0) < 1:
        raise BuildError(f"{case_id} must settle for at least one complete field")
    if settle.get("capture_fields", 0) < 2:
        raise BuildError(f"{case_id} must capture adjacent fields")
    if settle.get("ready_timeout_fields", 0) <= settle["settle_fields"]:
        raise BuildError(f"{case_id} ready timeout must exceed its settle count")

    applicability = case.get("applicability", {})
    if applicability.get("regions") != ["PAL"]:
        raise BuildError(f"{case_id} must declare the controlled PAL region")
    if applicability.get("min_chip_ram_bytes", 0) < 0x40000:
        raise BuildError(f"{case_id} needs chip RAM through the payload address")

    expected_schedule = {
        "phase": (
            "once per field through a Copper list after the ready record is published"
        ),
        "write_order": [
            "static register setup",
            "COP1LC",
            "DMACON Copper enable",
            "COPJMP1",
            "line 127 baseline reset",
            "line 128 COLOR00 marker",
            "line 128 tested register write",
        ],
        "steady_state": (
            "The Copper restores the initial register and guard colour on beam "
            "line 127, then applies one marked test write on beam line 128 in "
            "every field."
        ),
    }
    if case.get("programming_schedule") != expected_schedule:
        raise BuildError(f"{case_id}.programming_schedule changed")


def load_cases() -> tuple[dict[str, Any], list[dict[str, Any]]]:
    source = json.loads(CASE_FILE.read_text(encoding="utf-8"))
    if source.get("source_format_version") != 1:
        raise BuildError("unsupported cases.json source_format_version")
    suite = source.get("suite")
    required_suite = {"id", "version", "license", "source_revision"}
    if not isinstance(suite, dict) or set(suite) != required_suite:
        raise BuildError("suite metadata has the wrong fields")
    if suite["license"] != "CC0-1.0":
        raise BuildError("suite license must be CC0-1.0")
    expected_suite = {
        "id": SUITE_ID,
        "version": SUITE_VERSION,
        "license": "CC0-1.0",
        "source_revision": SOURCE_REVISION,
    }
    if suite != expected_suite:
        raise BuildError(f"suite metadata must be {expected_suite}")

    defaults = source.get("defaults")
    source_cases = source.get("cases")
    if not isinstance(defaults, dict) or not isinstance(source_cases, list):
        raise BuildError("cases.json must contain defaults and a cases array")

    cases = [merge_case(defaults, case) for case in source_cases]
    for case in cases:
        validate_case(case)

    ids = [case["id"] for case in cases]
    numeric_ids = [case["numeric_id"] for case in cases]
    if len(ids) != len(set(ids)) or len(numeric_ids) != len(set(numeric_ids)):
        raise BuildError("case ids and numeric ids must be unique")
    if numeric_ids != sorted(numeric_ids):
        raise BuildError("cases must be ordered by numeric_id")
    if tuple(ids) != CASE_IDS or numeric_ids != list(range(1, len(CASE_IDS) + 1)):
        raise BuildError("suite 1.0.0 must retain its five ordered case identities")
    return suite, cases


def resolve_tool(command: str) -> Path:
    found = shutil.which(command)
    if found is None:
        raise BuildError(f"required tool not found on PATH: {command}")
    return Path(found).resolve()


def run(command: list[str], *, env: dict[str, str] | None = None) -> str:
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


def tool_version(tool: Path) -> str:
    output = run([str(tool), "--version"])
    first_line = output.splitlines()
    if not first_line:
        raise BuildError(f"{tool} returned an empty version")
    return first_line[0].strip()


def generated_case_include(case: dict[str, Any]) -> str:
    registers = case["registers"]
    bplcon0_access = int(registers["bplcon0"]["word"], 16) | 0x0001
    timed_write = case["timed_write"]
    reset_wait = (
        (timed_write["reset_beam_line"] << 8)
        | (timed_write["reset_hpos_cck"] & 0xFE)
        | 1
    )
    timed_wait = (
        (timed_write["beam_line"] << 8)
        | (timed_write["wait_hpos_cck"] & 0xFE)
        | 1
    )
    baseline_word = registers[TIMED_REGISTERS[timed_write["register"]]]["word"]
    serial = case["identity"]["serial"]["value"]
    lines = [
        "/* SPDX-License-Identifier: CC0-1.0 */",
        "/* Generated from cases/cases.json; do not edit. */",
        f".equ CASE_NUMBER, {case['numeric_id']}",
        f".equ CASE_BPLCON0, {registers['bplcon0']['word']}",
        f".equ CASE_BPLCON0_ACCESS, 0x{bplcon0_access:04x}",
        f".equ CASE_BPLCON3, {registers['bplcon3']['word']}",
        f".equ CASE_BEAMCON0, {registers['beamcon0']['word']}",
        f".equ CASE_HBSTRT, {registers['hbstrt']['word']}",
        f".equ CASE_HBSTOP, {registers['hbstop']['word']}",
        f".equ CASE_COLOR00, {case['identity']['visual']['color00']}",
        f".equ CASE_MARKER_COLOR00, {case['identity']['visual']['marker_color00']}",
        f".equ CASE_RESET_WAIT, 0x{reset_wait:04x}",
        f".equ CASE_TIMED_WAIT, 0x{timed_wait:04x}",
        f".equ CASE_TIMED_REGISTER, {timed_write['register']}",
        f".equ CASE_BASELINE_WORD, {baseline_word}",
        f".equ CASE_TIMED_WORD, {timed_write['word']}",
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
    with tempfile.TemporaryDirectory(prefix="hblank-build-") as temporary:
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
                    "version": tool_version(assembler),
                },
                "linker": {
                    "command": linker.name,
                    "version": tool_version(linker),
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
        description="Build the emulator-neutral programmable-HBLANK corpus."
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
