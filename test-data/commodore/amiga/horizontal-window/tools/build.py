#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build horizontal display-window diagnostics using the existing SPHX guest."""

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
        "--resolution", choices=("lores", "hires", "superhires"), default="superhires"
    )
    args = parser.parse_args()
    assembler = shutil.which("m68k-elf-as")
    linker = shutil.which("m68k-elf-ld")
    if not assembler or not linker:
        parser.error("m68k-elf-as and m68k-elf-ld are required")
    sys.path.insert(0, str(CORPUS / "tools"))
    spec = importlib.util.spec_from_file_location(
        "sphx_build", CORPUS / "tools/build.py"
    )
    assert spec is not None and spec.loader is not None
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    _, cases = build.load_cases()
    original = (CORPUS / "src/probe.S").read_text()
    profiles = [
        ("legacy", None, None),
        ("explicit-legacy", 0x2000, None),
        ("stop-low-half", 0x0000, None),
        ("start-high-half", 0x2020, None),
        ("start-fine-1", 0x2008, None),
        ("start-fine-2", 0x2010, None),
        ("start-fine-3", 0x2018, None),
        ("stop-fine-1", 0x2800, None),
        ("stop-fine-2", 0x3000, None),
        ("stop-fine-3", 0x3800, None),
        ("visible-stop-0", 0x2000, None),
        ("visible-stop-1", 0x2800, None),
        ("visible-stop-2", 0x3000, None),
        ("visible-stop-3", 0x3800, None),
        ("rewrite-start", None, (0x08E, 0x2C81, 0x50)),
        ("rewrite-stop", None, (0x090, 0xF401, 0x70)),
        ("rewrite-high", 0x2000, (0x1E4, 0x0000, 0x50)),
    ]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    records = []
    for name, high, rewrite in profiles:
        run = output / name
        source = run / "src"
        source.mkdir(parents=True, exist_ok=True)
        case = copy.deepcopy(cases[0])
        case["id"] = name
        case["identity"]["serial"] = f"amiga-horizontal-window-v1/{name}"
        if args.resolution != "superhires":
            case["identity"]["serial"] = f"amiga-window-{args.resolution}-v1/{name}"
        start = 0x2CC1 if name == "rewrite-start" else 0x2C81
        stop = 0xF4A1 if name.startswith("visible-stop-") else 0xF4C1
        case["registers"].update(
            diwstrt=f"0x{start:04x}",
            diwstop=f"0x{stop:04x}",
            bplcon0={"lores": "0x1000", "hires": "0x9000", "superhires": "0x1040"}[
                args.resolution
            ],
            bplcon1="0x0000",
            bplcon3="0x00c0",
            bplcon4="0x0011",
            fmode="0x0000",
            dmacon_enable="0x8380",
            color00="0x0000",
            color01="0x0fff",
        )
        case["geometry"].update(bitplane_words_per_row=80, marker_word_index=16)
        for filename in ("custom-registers.inc", "bootblock.S"):
            shutil.copyfile(CORPUS / "src" / filename, source / filename)
        anchor = "    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)"
        assert original.count(anchor) == 1
        probe = original.replace(
            anchor,
            "    move.l #copper_list, %d0\n"
            "    move.w %d0, 0x082(%a6)\n    swap %d0\n"
            "    move.w %d0, 0x080(%a6)\n"
            "    move.w #CASE_DMACON_ENABLE, DMACON(%a6)\n"
            "    move.w #0, 0x088(%a6)",
        )
        probe = probe[: probe.index(".align 2\nbitplane_data:")]
        probe += (
            ".balign 8\nbitplane_data:\n    .rept 256*80\n    .word 0xffff\n    .endr\n"
        )
        probe += ".balign 4\ncopper_list:\n"
        address = f"(bitplane_data - _start + 0x{build.LOAD_ADDRESS:x})"
        for line in range(44, 244):
            probe += f"    .word 0x{(line << 8) | 0x11:04x}, 0xfffe\n"
            probe += f"    .word 0x00e0, ({address} >> 16)\n    .word 0x00e2, ({address} & 0xffff)\n"
            probe += (
                f"    .word 0x008e, 0x{start:04x}\n    .word 0x0090, 0x{stop:04x}\n"
            )
            if high is not None:
                probe += f"    .word 0x01e4, 0x{high:04x}\n"
            if rewrite is not None and 128 <= line < 144:
                register, value, first_wait = rewrite
                wait = first_wait + 2 * (line - 128)
                probe += f"    .word 0x{(line << 8) | wait | 1:04x}, 0xfffe\n"
                probe += f"    .word 0x{register:04x}, 0x{value:04x}\n"
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
                "diwhigh": high,
                "rewrite": rewrite,
                "adf_sha256": hashlib.sha256(adf).hexdigest(),
                "payload_sha256": hashlib.sha256(payload).hexdigest(),
                "source_sha256": {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in source.iterdir()
                },
            }
        )
    (output / "diagnostics.json").write_text(
        json.dumps(
            {
                "status": "software diagnostic; not hardware conformance evidence",
                "builder_sha256": hashlib.sha256(
                    Path(__file__).read_bytes()
                ).hexdigest(),
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Built {len(records)} horizontal-window diagnostics in {output}")


if __name__ == "__main__":
    main()
