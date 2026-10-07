#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build unadmitted, DMA-fed AGA sprite diagnostics with padded control words and addressed lanes."""

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
    records = []
    for mode in range(4):
        width = (1, 2, 2, 4)[mode]
        for offset in (0, 2, 4, 6):
            name = f"fmode{mode}-offset{offset}"
            run = output / name
            run.mkdir(exist_ok=True)
            case = copy.deepcopy(cases[0])
            case["id"] = name
            case["identity"]["serial"] = f"amiga-wide-sprite-diagnostic-v1/{name}"
            case["registers"].update(
                fmode=f"0x{mode << 2:04x}",
                bplcon3="0x00c0",
                spr0ctl="0x9008",
                spr0data="0xa5a5",
                spr0datb="0x0000",
            )
            case["geometry"].update(
                resolution="lores", bitplane_words_per_row=20, marker_word_index=4
            )
            source = run / "src"
            source.mkdir(exist_ok=True)
            for filename in ("custom-registers.inc", "bootblock.S"):
                shutil.copyfile(CORPUS / "src" / filename, source / filename)
            data = [
                ".balign 8",
                *([f"    .space {offset}"] if offset else []),
                "sprite_data:",
                "    .word CASE_SPR0POS",
            ]
            data += ["    .word 0xdead"] * (width - 1)
            data += ["    .word CASE_SPR0CTL"] + ["    .word 0xbeef"] * (width - 1)
            lanes = (0xA5A5, 0x5A5A, 0x0F0F, 0xF0F0)
            for line in range(16):
                data += [
                    f"    .word 0x{(word ^ (line * 0x0101)):04x}"
                    for word in lanes[:width]
                ]
                data += ["    .word 0x0000"] * width
            data += ["    .word 0x0000"] * (width * 2)
            data += [".balign 8", "empty_sprite:"] + ["    .word 0x0000"] * (width * 2)
            start = original.index(".align 2\nsprite_data:")
            end = original.index(".align 2\nbitplane_data:", start)
            (source / "probe.S").write_text(
                original[:start] + "\n".join(data) + "\n\n" + original[end:]
            )
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
                    "sprite_fmode": mode,
                    "address_offset": offset,
                    "transfer_words": width,
                    "source_sha256": {
                        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                        for p in source.iterdir()
                    },
                    "data_lane_words": [f"0x{word:04x}" for word in lanes[:width]],
                    "payload_bytes": len(payload),
                    "adf_sha256": hashlib.sha256(adf).hexdigest(),
                    "payload_sha256": hashlib.sha256(payload).hexdigest(),
                }
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
