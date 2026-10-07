#!/usr/bin/env python3
"""Build neutral SPHX guests around Lisa's pending counter reset."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh output directory")
    args = parser.parse_args()
    base = Path(__file__).resolve().parents[1]
    root = Path(__file__).resolve().parents[6]
    seed = base / "seed"
    tools = root / "test-data/commodore/amiga/sprite-horizontal-phase/tools"
    sys.path.insert(0, str(tools))
    spec = importlib.util.spec_from_file_location("sphx_build", tools / "build.py")
    if spec is None or spec.loader is None:
        raise ValueError("missing existing SPHX builder")
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    assembler = shutil.which("m68k-elf-as")
    linker = shutil.which("m68k-elf-ld")
    if not assembler or not linker:
        parser.error("m68k-elf-as and m68k-elf-ld are required")
    output = args.output.resolve()
    output.mkdir(parents=True)
    records = []
    for edge in ("start", "stop"):
        for word in [0, *[1 + (fine << 8) for fine in range(8)], 2]:
            name = f"{edge}-{word:04x}"
            case_dir = output / "guests" / name
            case_dir.mkdir(parents=True)
            shutil.copytree(seed / "src", case_dir / "src")
            case = json.loads((seed / "inputs.json").read_text())
            case["id"] = name
            case["identity"]["serial"] = "amiga-blank-reset-v1/" + name
            case["question"] = (
                f"What output is observed when the {edge} comparator word is "
                f"${word:04X} at the strobe reset?"
            )
            case["registers"]["hbstrt"] = f"0x{word if edge == 'start' else 0x80:04x}"
            case["registers"]["hbstop"] = f"0x{word if edge == 'stop' else 0xA0:04x}"
            source = (seed / "src/probe.S").read_text()
            for key, old, address in (
                ("hbstrt", "0x0080", "1c4"),
                ("hbstop", "0x00a0", "1c6"),
            ):
                needle = f"move.w  #{old}, 0x{address}(%a6)"
                if source.count(needle) != 1:
                    raise ValueError("seed register write is missing or ambiguous")
                source = source.replace(
                    needle, f"move.w  #{case['registers'][key]}, 0x{address}(%a6)"
                )
            (case_dir / "src/probe.S").write_text(source)
            (case_dir / "inputs.json").write_text(json.dumps(case, indent=2) + "\n")
            build.SOURCE_DIR = case_dir / "src"
            payload = build.assemble_payload(
                case, case_dir, Path(assembler), Path(linker), os.environ.copy()
            )
            boot = build.assemble_bootblock(
                (len(payload) + 511) // 512,
                case_dir,
                Path(assembler),
                Path(linker),
                os.environ.copy(),
            )
            adf, _, _ = build.pack_adf(boot, payload)
            (case_dir / "probe.adf").write_bytes(adf)
            records.append(
                {"case": name, "adf_sha256": hashlib.sha256(adf).hexdigest()}
            )
    (output / "cases.json").write_text(json.dumps(records, indent=2) + "\n")
    print(f"Built {len(records)} cases")


if __name__ == "__main__":
    main()
