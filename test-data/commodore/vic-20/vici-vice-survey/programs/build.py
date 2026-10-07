#!/usr/bin/env python3
"""Build the VIC-20 VIC-I survey's raster programs.

The four `raster-*.a` programs are this project's own work, so their images
sit in the repository beside their source. They are inputs to the VIC-I
survey against VICE (`knowledge/processes/vic20-vici-vice-survey.md`), which
pins each image's SHA-256 in `../cases-v1.json`; what each program draws and
why is explained at the top of its source.

Assembly uses this project's own assembler. Run with no arguments to write
the images; `--check` rebuilds and compares instead, for CI.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROGRAMS = ("raster-border", "raster-background", "raster-reverse", "raster-auxiliary")


def assemble(name: str) -> bytes:
    source = HERE / f"{name}.a"
    out = HERE / f"{name}.tmp.prg"
    try:
        subprocess.run(
            ["asm198x", "asm", "--dialect", "acme", "--prg", str(source), "-o", str(out)],
            check=True,
            capture_output=True,
            text=True,
            cwd=HERE,
        )
        image = out.read_bytes()
    finally:
        out.unlink(missing_ok=True)
    if image[:2] != b"\x01\x10":
        raise SystemExit(f"{source.name} does not load at $1001")
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare instead of writing")
    args = parser.parse_args()

    stale = []
    for name in PROGRAMS:
        image = assemble(name)
        target = HERE / f"{name}.prg"
        if args.check:
            if not target.exists() or target.read_bytes() != image:
                stale.append(target.name)
            continue
        target.write_bytes(image)
        print(f"wrote {target.name} ({len(image)} bytes)")
    if stale:
        print(f"{', '.join(stale)} do not match their source; rerun without --check", file=sys.stderr)
        return 1
    if args.check:
        print(f"{len(PROGRAMS)} raster programs match their source")
    return 0


if __name__ == "__main__":
    sys.exit(main())
