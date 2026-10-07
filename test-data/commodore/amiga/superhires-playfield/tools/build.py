#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build unadmitted, DMA-fed AGA playfield diagnostics with resolution-sized rows."""

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
PROFILES = (
    ("lores", 0x1000, 0, 20, 4),
    ("hires", 0x9000, 0, 40, 8),
    ("shres-fmode0", 0x1040, 0, 80, 16),
    ("shres-fmode1", 0x1040, 1, 80, 16),
    ("shres-fmode3", 0x1040, 3, 80, 16),
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--word", type=lambda value: int(value, 0), default=0x8000)
    args = parser.parse_args()
    if not 0 <= args.word <= 0xFFFF:
        parser.error("--word must be a 16-bit value")
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
    source = output / "src"
    source.mkdir(parents=True, exist_ok=True)
    for name in ("probe.S", "custom-registers.inc", "bootblock.S"):
        shutil.copyfile(CORPUS / "src" / name, source / name)
    probe = source / "probe.S"
    probe.write_text(
        probe.read_text().replace(
            ".word (1 << CASE_MARKER_BIT_INDEX)", f".word 0x{args.word:04x}"
        )
    )
    build.SOURCE_DIR = source
    records = []
    for name, mode, fmode, words, marker in PROFILES:
        case = copy.deepcopy(cases[0])
        case["id"] = name
        case["identity"]["serial"] = f"amiga-playfield-diagnostic-v1/{name}"
        case["registers"].update(
            bplcon0=f"0x{mode:04x}",
            fmode=f"0x{fmode:04x}",
            bplcon3="0x00c0",
            spr0ctl="0x9008",
            spr0data="0xa5a5",
        )
        case["geometry"].update(
            resolution="shres" if mode & 0x40 else name,
            bitplane_words_per_row=words,
            marker_word_index=marker,
        )
        run = output / name
        run.mkdir(exist_ok=True)
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
                "row_words": words,
                "rows": 256,
                "marker_word": f"0x{args.word:04x}",
                "payload_bytes": len(payload),
                "adf_sha256": hashlib.sha256(adf).hexdigest(),
                "payload_sha256": hashlib.sha256(payload).hexdigest(),
            }
        )
    manifest = {
        "status": "diagnostic; not admitted hardware conformance evidence",
        "source": str(CORPUS.relative_to(Path(__file__).resolve().parents[5])),
        "source_sha256": {
            p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in source.iterdir()
        },
        "cases": records,
    }
    (output / "diagnostics.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Built {len(records)} diagnostics in {output}")


if __name__ == "__main__":
    main()
