#!/usr/bin/env python3
"""Derive neutral counter-traced blanking probes from the existing colour guest."""

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("colour_guest", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[5]
tools = root / "test-data/commodore/amiga/sprite-horizontal-phase/tools"
sys.path.insert(0, str(tools))
s = importlib.util.spec_from_file_location("build", tools / "build.py")
m = importlib.util.module_from_spec(s)
s.loader.exec_module(m)
source = args.colour_guest
cases = json.loads(
    (
        root / "test-data/commodore/amiga/programmable-hblank/cases/cases.json"
    ).read_text()
)["cases"]
for model in ("ecs", "aga"):
    for c in cases:
        if model == "ecs" and c["id"].startswith("aga-"):
            continue
        out = args.output.resolve() / model / "guests" / c["id"]
        out.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source / "src", out / "src", dirs_exist_ok=True)
        case = json.loads((source / "inputs.json").read_text())
        case["id"] = c["id"]
        case["identity"]["serial"] = "amiga-counter-blank-v1/" + c["id"]
        case["question"] = c["question"]
        for k, v in c["registers"].items():
            case["registers"][k] = v["word"]
        case["registers"]["bplcon0"] = (
            f"0x{int(case['registers']['bplcon0'], 16) | 0x1000:04x}"
        )
        case["registers"]["color00"] = "0x0011"
        case["geometry"]["resolution"] = c["resolution"]
        case["programming_schedule"] = {
            "phase": "CPU before Copper and ready record",
            "steady_state": "Fixed blanking registers; Copper resets COLOR00 to 0x011 on lines 44..243.",
        }
        (out / "inputs.json").write_text(json.dumps(case, indent=2) + "\n")
        probe = (source / "src/probe.S").read_text()
        for line in range(128, 136):
            seq = f"    .word 0x{(line << 8) | 0x91:04x}, 0xfffe\n" + "".join(
                f"    .word 0x0180, 0x{v:04x}\n" for v in (0xF00, 0x0F0, 0x00F, 0xFF0)
            )
            assert probe.count(seq) == 1
            probe = probe.replace(seq, "")
        anchor = "    move.w  #CASE_BPLCON0, BPLCON0(%a6)"
        assert probe.count(anchor) == 1
        writes = "".join(
            f"    move.w  #{case['registers'][k]}, 0x{addr:03x}(%a6)\n"
            for k, addr in [("hbstrt", 0x1C4), ("hbstop", 0x1C6), ("beamcon0", 0x1DC)]
        )
        probe = probe.replace(anchor, writes + anchor)
        (out / "src/probe.S").write_text(probe)
        m.SOURCE_DIR = out / "src"
        a = Path(shutil.which("m68k-elf-as"))
        ld = Path(shutil.which("m68k-elf-ld"))
        payload = m.assemble_payload(case, out, a, ld, os.environ.copy())
        boot = m.assemble_bootblock(
            (len(payload) + 511) // 512, out, a, ld, os.environ.copy()
        )
        adf, _, _ = m.pack_adf(boot, payload)
        (out / "probe.adf").write_bytes(adf)
        print(model, c["id"], hashlib.sha256(adf).hexdigest())
