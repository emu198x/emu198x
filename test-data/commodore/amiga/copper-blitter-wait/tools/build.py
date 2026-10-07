#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build paired AGA PAL diagnostics for Copper WAIT after a blitter start."""

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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    corpus = Path(__file__).resolve().parents[2] / "sprite-horizontal-phase"
    sys.path.insert(0, str(corpus / "tools"))
    spec = importlib.util.spec_from_file_location(
        "probe_builder", corpus / "tools/build.py"
    )
    assert spec and spec.loader
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    _, cases = build.load_cases()
    assert cases
    assembler = shutil.which("m68k-elf-as")
    linker = shutil.which("m68k-elf-ld")
    assert assembler and linker
    records = []
    for bfd in [False, True]:
        name = "bfd-ignore" if bfd else "bfd-wait"
        run = output / name
        run.mkdir(exist_ok=True)
        source = run / "src"
        source.mkdir(exist_ok=True)
        case = copy.deepcopy(cases[0])
        case["id"] = name
        case["identity"]["serial"] = "amiga-copper-blitwait-v1/" + name
        case["registers"].update(
            bplcon0="0x0000", fmode="0x0000", dmacon_enable="0x82c0"
        )
        case["question"] = (
            "When does a completion-dependent Copper WAIT reach its first MOVE?"
        )
        for filename in ["custom-registers.inc", "bootblock.S"]:
            shutil.copyfile(corpus / "src" / filename, source / filename)
        probe = (
            (corpus / "src/probe.S")
            .read_text()
            .replace(
                "    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)",
                "    move.w #2, 0x02e(%a6)\n    move.l #copper_list, %d0\n    move.w %d0, 0x082(%a6)\n    swap %d0\n    move.w %d0, 0x080(%a6)\n    move.w #CASE_DMACON_ENABLE, DMACON(%a6)\n    move.w #0, 0x088(%a6)",
            )
        )
        probe = (
            probe[: probe.index(".align 2\nbitplane_data:")]
            + ".align 2\nbitplane_data:\n    .word 0\n.balign 4\ncopper_list:\n"
        )
        schedules = []
        for index in range(48):
            line = 60 + index * 3
            width = [1, 2, 4, 8, 16, 32, 64, 128][index % 8]
            hpos = [0x40, 0x60, 0x80][(index // 8) % 3]
            nasty = index >= 24
            probe += f"    .word 0x{(line << 8) | hpos | 1:04x}, 0xfffe\n"
            for reg, val in [
                (0x180, 0),
                (0x096, 0x8400 if nasty else 0x0400),
                (0x040, 0x01FF),
                (0x042, 0),
                (0x066, 0),
                (0x054, 4),
                (0x056, 0),
                (0x058, ((width + 63) // 64) * 64 + (width % 64)),
            ]:
                probe += f"    .word 0x{reg:04x}, 0x{val:04x}\n"
            probe += f"    .word 0x{(line << 8) | hpos | 1:04x}, 0x{0xFFFE if bfd else 0x7FFE:04x}\n    .word 0x0180, 0x0f00\n"
            # Wait until two lines later, so long fills remain observable.
            probe += f"    .word 0x{((line + 2) << 8) | 0x21:04x}, 0xfffe\n    .word 0x0180, 0\n"
            schedules.append(
                {"line": line, "words": width, "hpos": hpos, "nasty": nasty, "bfd": bfd}
            )
        probe += "    .word 0xffff, 0xfffe\n.size _start, .-_start\n"
        (source / "probe.S").write_text(probe)
        (run / "inputs.json").write_text(json.dumps(case, indent=2) + "\n")
        build.SOURCE_DIR = source
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
                "schedules": schedules,
                "adf_sha256": hashlib.sha256(adf).hexdigest(),
                "source_sha256": hashlib.sha256(probe.encode()).hexdigest(),
            }
        )
    assert len(records) == 2
    (output / "diagnostics.json").write_text(
        json.dumps(
            {
                "status": "unadmitted diagnostic; BFD=0 retains a completion/wake timing mismatch",
                "builder_sha256": hashlib.sha256(
                    Path(__file__).read_bytes()
                ).hexdigest(),
                "cases": records,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
