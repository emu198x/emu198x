#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build unadmitted, AGA diagnostics for Copper writes to independent playfield scrolling."""

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
        "--scroll-sweep",
        action="store_true",
        help="Sweep static extended/fractional offsets and all distinct fetch widths",
    )
    parser.add_argument(
        "--ddf-start",
        type=lambda value: int(value, 0),
        choices=(0x30, 0x38),
        default=0x38,
    )
    args = parser.parse_args()
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
    profiles = tuple(
        (f"{mode}-f{fmode}-{kind}", resolution | 0x2400, fmode, 0x102, 0x0035)
        for mode, resolution in (("lores", 0), ("hires", 0x8000), ("shres", 0x0040))
        for fmode in (0, 1, 3)
        for kind in ("control", "changes")
    )
    records = []
    for name, resolution, fmode, register, before in profiles:
        run = output / name
        run.mkdir(exist_ok=True)
        case = copy.deepcopy(cases[0])
        case["id"] = name
        case["identity"]["serial"] = (
            f"amiga-scroll-sweep-v1/{name}"
            if args.scroll_sweep
            else f"amiga-midline-scroll-v3/{name}"
        )
        if args.ddf_start != 0x38:
            case["identity"]["serial"] = (
                f"amiga-scroll-sweep-v1/ddf{args.ddf_start:02x}/{name}"
            )
        case["question"] = (
            "Do independent mid-line playfield scroll changes agree with the reference?"
        )
        case["programming_schedule"] = {
            "phase": "Initial registers with DMA disabled; Copper resets both pointers and BPLCON1 before each visible line.",
            "steady_state": "Lines 128 through 159 change BPLCON1 at 32 distinct Copper WAIT phases; other lines retain the initial offset.",
        }
        if args.scroll_sweep:
            case["question"] = (
                "Do independent extended and fractional static scroll offsets agree with the reference?"
            )
            case["programming_schedule"]["steady_state"] = (
                "Lines 128 through 191 program static per-line offsets covering extended integer and fractional fields; no mid-line BPLCON1 write. Other lines retain $0035."
            )
        case["identity"].update(
            marker="green PF1",
            sprite="sprites disabled",
            playfield2="orange PF2, palette entry 9",
        )
        case["registers"].update(
            ddfstrt=f"0x{args.ddf_start:04x}",
            bplcon0=f"0x{resolution:04x}",
            fmode=f"0x{fmode:04x}",
            bplcon3="0x0cc0",
            bplcon1="0x0035",
            bplcon2="0x0000",
            bplcon4="0x0000",
            dmacon_enable="0x8380",
        )
        case["geometry"].update(
            resolution=name.split("-")[0],
            bitplane_words_per_row=80,
            bitplane_rows=1,
            marker_word_index=16,
            anchors=[
                "retained hardwired HBLANK",
                "independent asymmetric PF1 and PF2 DMA streams",
            ],
        )
        source = run / "src"
        source.mkdir(exist_ok=True)
        for filename in ("custom-registers.inc", "bootblock.S"):
            shutil.copyfile(CORPUS / "src" / filename, source / filename)
        probe = original.replace(
            "    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)",
            "    move.l #copper_list, %d0\n    move.w %d0, 0x082(%a6)\n    swap %d0\n    move.w %d0, 0x080(%a6)\n    move.w #CASE_DMACON_ENABLE, DMACON(%a6)\n    move.w #0, 0x088(%a6)",
        )
        start = probe.index(".align 2\nbitplane_data:")
        probe = probe[:start].replace(
            "    lea     sprite_data(%pc), %a1",
            "    move.l #bitplane_even, %d0\n    move.w %d0, 0x0e6(%a6)\n    swap %d0\n    move.w %d0, 0x0e4(%a6)\n    lea sprite_data(%pc), %a1",
        )
        probe = probe.replace(
            "    move.w  #CASE_COLOR17, COLOR17(%a6)",
            "    move.w #0x0200, BPLCON3(%a6)\n    move.w #0, 0x192(%a6)\n    move.w #CASE_BPLCON3, BPLCON3(%a6)\n    move.w #0x0f80, 0x192(%a6)\n    move.w #CASE_COLOR17, COLOR17(%a6)",
        )
        for label, seed, multiplier in (
            ("bitplane_data", 0xA5A5, 0x1F3D),
            ("bitplane_even", 0x6C39, 0x37B1),
        ):
            probe += f".balign 8\n{label}:\n"
            probe += "".join(
                f"    .word 0x{((index * multiplier) ^ seed) & 0xFFFF:04x}\n"
                for index in range(80)
            )
        probe += "\n.balign 4\ncopper_list:\n"
        scroll_by_line = {}
        lines = range(44, 244)
        for line in lines:
            probe += f"    .word 0x{(line << 8) | 0x21:04x}, 0xfffe\n"
            address = f"(bitplane_data - _start + 0x{build.LOAD_ADDRESS:x})"
            probe += f"    .word 0x00e0, ({address} >> 16)\n    .word 0x00e2, ({address} & 0xffff)\n"
            address_even = f"(bitplane_even - _start + 0x{build.LOAD_ADDRESS:x})"
            probe += f"    .word 0x00e4, ({address_even} >> 16)\n    .word 0x00e6, ({address_even} & 0xffff)\n"
            line_scroll = before
            if args.scroll_sweep and name.endswith("changes") and 128 <= line < 192:
                r = {"lores": 0, "hires": 1, "shres": 2}[name.split("-")[0]]
                delay = (line - 128) & ((64 >> r) - 1)
                even = (delay * 7 + 3) & ((64 >> r) - 1)
                fraction = ((line - 128) >> (6 - r)) << (2 - r)
                line_scroll = (
                    (delay & 15)
                    | ((delay & 48) << 6)
                    | ((even & 15) << 4)
                    | ((even & 48) << 10)
                    | (fraction << 8)
                    | (fraction << 12)
                )
            scroll_by_line[line] = f"0x{line_scroll:04x}"
            probe += f"    .word 0x{register:04x}, 0x{line_scroll:04x}\n"
            if not args.scroll_sweep and 128 <= line < 160:
                wait = 0x60 + (line - 128) * 2
                changed = (
                    before
                    if name.endswith("control")
                    else (0x003B, 0x00C5, 0x00CB)[(line - 128) % 3]
                )
                probe += f"    .word 0x{(line << 8) | wait | 1:04x}, 0xfffe\n    .word 0x{register:04x}, 0x{changed:04x}\n"
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
                "pattern": "independent asymmetric words",
                "reset_each_line": True,
                "register": f"0x{register:04x}",
                "before": f"0x{before:04x}",
                "scroll_by_line": scroll_by_line,
                "scroll_sweep": args.scroll_sweep,
                "after_by_line": []
                if args.scroll_sweep
                else [
                    f"0x{(before if name.endswith('control') else (0x003B, 0x00C5, 0x00CB)[index % 3]):04x}"
                    for index in range(32)
                ],
                "source_sha256": {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in source.iterdir()
                },
                "payload_bytes": len(payload),
                "adf_sha256": hashlib.sha256(adf).hexdigest(),
                "payload_sha256": hashlib.sha256(payload).hexdigest(),
            }
        )
        records[-1].update(
            ddf_start=args.ddf_start,
            changed_lines=[128, 192] if args.scroll_sweep else [128, 160],
            copper_wait_ccks=[] if args.scroll_sweep else list(range(0x60, 0xA0, 2)),
            extra_palette={"color09": "0x0f80"},
            bitplane_patterns=[
                {"seed": "0xa5a5", "multiplier": "0x1f3d"},
                {"seed": "0x6c39", "multiplier": "0x37b1"},
            ],
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
