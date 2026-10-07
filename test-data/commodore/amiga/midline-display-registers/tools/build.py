#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build unadmitted, AGA diagnostics for Copper writes to resolution, fetch mode and palette XOR."""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
import shutil
import sys
from pathlib import Path

CORPUS = Path(__file__).resolve().parents[2] / "sprite-horizontal-phase"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--pattern", choices=("constant", "varying"), default="constant"
    )
    parser.add_argument("--reset-each-line", action="store_true")
    parser.add_argument("--phase-sweep", action="store_true")
    parser.add_argument(
        "--ddf-start",
        type=lambda value: int(value, 0),
        choices=(0x30, 0x38),
        default=0x38,
    )
    args = parser.parse_args()
    if args.phase_sweep and (args.pattern != "varying" or not args.reset_each_line):
        parser.error("--phase-sweep requires --pattern varying --reset-each-line")
    if not args.phase_sweep and args.ddf_start != 0x38:
        parser.error("a different --ddf-start requires --phase-sweep")
    assembler = shutil.which("m68k-elf-as")
    linker = shutil.which("m68k-elf-ld")
    if not assembler or not linker:
        parser.error("m68k-elf-as and m68k-elf-ld are required")
    sys.path.insert(0, str(CORPUS / "tools"))
    spec = importlib.util.spec_from_file_location(
        "sprite_probe_build", CORPUS / "tools/build.py"
    )
    assert spec is not None and spec.loader is not None
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    _, cases = build.load_cases()  # Validate the canonical input before forking it.
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    original = (CORPUS / "src/probe.S").read_text()
    profiles = (
        ("control", 0x1000, 0, 0x100, 0x1000, 0x1000),
        ("lores-hires", 0x1000, 0, 0x100, 0x1000, 0x9000),
        ("hires-lores", 0x9000, 0, 0x100, 0x9000, 0x1000),
        ("hires-shres", 0x9000, 0, 0x100, 0x9000, 0x1040),
        ("shres-hires", 0x1040, 0, 0x100, 0x1040, 0x9000),
        ("fetch-0-1", 0x1040, 0, 0x1FC, 0, 1),
        ("fetch-1-3", 0x1040, 1, 0x1FC, 1, 3),
        ("palette-xor", 0x1000, 0, 0x10C, 0x0011, 0x0111),
    )
    if args.phase_sweep:
        profiles = profiles[:-1] + (
            ("lores-shres", 0x1000, 0, 0x100, 0x1000, 0x1040),
            ("shres-lores", 0x1040, 0, 0x100, 0x1040, 0x1000),
            ("fetch-3-1", 0x1040, 3, 0x1FC, 3, 1),
            ("fetch-1-0", 0x1040, 1, 0x1FC, 1, 0),
            ("control-hires", 0x9000, 0, 0x100, 0x9000, 0x9000),
            ("control-shres", 0x1040, 0, 0x100, 0x1040, 0x1040),
        )
    records = []
    for name, resolution, fmode, register, before, after in profiles:
        run = output / name
        run.mkdir(exist_ok=True)
        case = copy.deepcopy(cases[0])
        case["id"] = name
        case["identity"]["serial"] = (
            f"amiga-midline-{args.pattern}-reset-v1/{name}"
            if args.reset_each_line
            else (
                f"amiga-midline-diagnostic-v1/{name}"
                if args.pattern == "constant"
                else f"amiga-midline-varying-v1/{name}"
            )
        )
        if args.phase_sweep:
            case["identity"]["serial"] = (
                f"amiga-phase-sweep-v1/ddf{args.ddf_start:02x}/{name}"
            )
        case["registers"].update(
            bplcon0=f"0x{resolution:04x}",
            fmode=f"0x{fmode:04x}",
            bplcon3="0x00c0",
            dmacon_enable="0x8380",
        )
        case["geometry"].update(bitplane_words_per_row=80, marker_word_index=16)
        if args.phase_sweep:
            case["registers"]["ddfstrt"] = f"0x{args.ddf_start:04x}"
        source = run / "src"
        source.mkdir(exist_ok=True)
        for filename in ("custom-registers.inc", "bootblock.S"):
            shutil.copyfile(CORPUS / "src" / filename, source / filename)
        probe = original.replace(
            "    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)",
            "    move.l #copper_list, %d0\n    move.w %d0, 0x082(%a6)\n    swap %d0\n    move.w %d0, 0x080(%a6)\n    move.w #CASE_DMACON_ENABLE, DMACON(%a6)\n    move.w #0, 0x088(%a6)",
        )
        start = probe.index(".align 2\nbitplane_data:")
        words = [
            0xA5A5
            if args.pattern == "constant"
            else ((index * 0x1F3D) ^ 0xA5A5) & 0xFFFF
            for index in range(80)
        ]
        probe = probe[:start] + ".balign 8\nbitplane_data:\n    .rept 256\n"
        probe += "".join(f"        .word 0x{word:04x}\n" for word in words)
        probe += "    .endr\n\n.balign 4\ncopper_list:\n"
        lines = range(44, 244) if args.reset_each_line else range(128, 144)
        for line in lines:
            probe += f"    .word 0x{(line << 8) | 0x21:04x}, 0xfffe\n"
            if args.reset_each_line:
                address = f"(bitplane_data - _start + 0x{build.LOAD_ADDRESS:x})"
                probe += f"    .word 0x00e0, ({address} >> 16)\n    .word 0x00e2, ({address} & 0xffff)\n"
            probe += f"    .word 0x{register:04x}, 0x{before:04x}\n"
            if 128 <= line < (160 if args.phase_sweep else 144):
                wait = 0x60 + (line - 128) * 2 if args.phase_sweep else 0x80
                probe += f"    .word 0x{(line << 8) | wait | 1:04x}, 0xfffe\n    .word 0x{register:04x}, 0x{after:04x}\n"
        if not args.reset_each_line:
            probe += f"    .word 0x9021, 0xfffe\n    .word 0x{register:04x}, 0x{before:04x}\n"
        probe += "    .word 0xffff, 0xfffe\n.size _start, .-_start\n"
        (source / "probe.S").write_text(probe)
        build.SOURCE_DIR = source
        (run / "inputs.json").write_text(json.dumps(case, indent=2) + "\n")
        payload = build.assemble_payload(
            case, run, Path(assembler), Path(linker), os.environ.copy()
        )
        boot = build.assemble_bootblock(
            (len(payload) + 511) // 512,
            run,
            Path(assembler),
            Path(linker),
            os.environ.copy(),
        )
        adf, _, _ = build.pack_adf(boot, payload)
        (run / "probe.adf").write_bytes(adf)
        records.append(
            {
                "case": name,
                "pattern": args.pattern,
                "reset_each_line": args.reset_each_line,
                "register": f"0x{register:04x}",
                "before": f"0x{before:04x}",
                "after": f"0x{after:04x}",
                "source_sha256": {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in source.iterdir()
                },
                "payload_bytes": len(payload),
                "adf_sha256": hashlib.sha256(adf).hexdigest(),
                "payload_sha256": hashlib.sha256(payload).hexdigest(),
            }
        )
        if args.phase_sweep:
            records[-1].update(
                ddf_start=args.ddf_start,
                changed_lines=[128, 160],
                copper_wait_ccks=list(range(0x60, 0xA0, 2)),
            )
    manifest = {
        "status": "diagnostic; not admitted hardware conformance evidence",
        "source": str(CORPUS.relative_to(Path(__file__).resolve().parents[5])),
        "builder_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "cases": records,
    }
    (output / "diagnostics.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Built {len(records)} diagnostics in {output}")


if __name__ == "__main__":
    main()
