#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Build paired A1200 PAL wrap diagnostics with deterministic DMA source RAM."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
COMMON = HERE.parent / "sprite-horizontal-phase"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
assembler = shutil.which("m68k-elf-as")
linker = shutil.which("m68k-elf-ld")
if not assembler or not linker:
    parser.error("m68k-elf-as and m68k-elf-ld are required")
sys.path.insert(0, str(COMMON / "tools"))
spec = importlib.util.spec_from_file_location("sprite_build", COMMON / "tools/build.py")
assert spec is not None and spec.loader is not None
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)
build.load_cases()
records = []
for name, beam in [("control", 0x0020), ("harddis", 0x4020)]:
    run = args.output.resolve() / name
    source = run / "src"
    source.mkdir(parents=True, exist_ok=True)
    case = json.loads((HERE / "wrap-inputs.json").read_text())
    case["identity"]["serial"] = "amiga-rga-wrap-controlled-v1/" + name
    probe = (HERE / "src/wrap.S").read_text().replace("@BEAMCON0@", f"0x{beam:04x}")
    assert "@BEAMCON0@" not in probe
    (source / "probe.S").write_text(probe)
    for filename in ["custom-registers.inc", "bootblock.S"]:
        shutil.copyfile(COMMON / "src" / filename, source / filename)
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
            "beamcon0": beam,
            "adf_sha256": hashlib.sha256(adf).hexdigest(),
            "source_sha256": {
                p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                for p in source.iterdir()
            },
        }
    )
(args.output / "diagnostics.json").write_text(
    json.dumps(
        {
            "status": "software-reference diagnostic; not hardware conformance",
            "cases": records,
        },
        indent=2,
    )
    + "\n"
)
print("Built two controlled wrap diagnostics")
