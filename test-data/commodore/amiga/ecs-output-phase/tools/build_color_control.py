#!/usr/bin/env python3
"""Derive an adjacent-COLOR00 control from the existing legacy window guest."""

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
    parser.add_argument("legacy_window_guest", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    source = args.legacy_window_guest
    output = args.output.resolve()
    tools = Path(__file__).resolve().parents[2] / "sprite-horizontal-phase/tools"
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
    output.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source / "src", output / "src", dirs_exist_ok=True)
    case = json.loads((source / "inputs.json").read_text())
    case["id"] = "color-moves"
    case["identity"]["serial"] = "amiga-counter-color-v1/color-moves"
    case["registers"]["bplcon0"] = "0x1000"
    (output / "inputs.json").write_text(json.dumps(case, indent=2) + "\n")
    probe = (source / "src/probe.S").read_text()
    pattern = ".word 0xffff\n    .endr"
    if probe.count(pattern) != 1:
        raise ValueError("legacy guest must contain one solid bitplane array")
    probe = probe.replace(pattern, ".word 0x0000\n    .endr")
    anchor = "    .word 0x0090, 0xf4c1\n"
    if probe.count(anchor) != 200:
        raise ValueError("legacy guest must reset DIW on all 200 active lines")
    parts = probe.split(anchor)
    result = parts[0]
    for line, part in zip(range(44, 244), parts[1:], strict=True):
        result += anchor + "    .word 0x0180, 0x0011\n"
        if 128 <= line < 136:
            result += f"    .word 0x{(line << 8) | 0x91:04x}, 0xfffe\n"
            for value in (0xF00, 0x0F0, 0x00F, 0xFF0):
                result += f"    .word 0x0180, 0x{value:04x}\n"
        result += part
    (output / "src/probe.S").write_text(result)
    build.SOURCE_DIR = output / "src"
    payload = build.assemble_payload(
        case, output, Path(assembler), Path(linker), os.environ.copy()
    )
    boot = build.assemble_bootblock(
        (len(payload) + 511) // 512,
        output,
        Path(assembler),
        Path(linker),
        os.environ.copy(),
    )
    adf, _, _ = build.pack_adf(boot, payload)
    (output / "probe.adf").write_bytes(adf)
    print(hashlib.sha256(adf).hexdigest())


if __name__ == "__main__":
    main()
