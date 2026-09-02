#!/usr/bin/env python3
"""Build the DLI timing probe cartridge for the Atari 800XL.

`atari-800xl-dli-timing.s` is a program this project wrote, so the image can sit
in the repository and CI can run it on every push with no ROM at all: the
800XL starts a cartridge directly when it has no OS. The test that drives it
is `crates/machine-atari-800xl/tests/dli_timing.rs`; what the cartridge does and
why is explained at the top of the source.

Assembly uses this project's own assembler. Run with no arguments to write
the image; `--check` rebuilds and compares instead, for CI.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "atari-800xl-dli-timing.s"
IMAGE = HERE / "atari-800xl-dli-timing.bin"
CART_SIZE = 8 * 1024


def assemble() -> bytes:
    out = SOURCE.with_suffix(".tmp.bin")
    try:
        subprocess.run(
            ["asm198x", "asm", "--dialect", "acme", str(SOURCE), "-o", str(out)],
            check=True,
            capture_output=True,
            text=True,
        )
        image = out.read_bytes()
    finally:
        out.unlink(missing_ok=True)
    if len(image) != CART_SIZE:
        raise SystemExit(f"{SOURCE.name} assembled to {len(image)} bytes, not {CART_SIZE}")
    return image


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare instead of writing")
    args = parser.parse_args()

    image = assemble()
    if args.check:
        if not IMAGE.exists() or IMAGE.read_bytes() != image:
            print(f"{IMAGE.name} does not match its source; rerun without --check", file=sys.stderr)
            return 1
        print(f"{IMAGE.name} matches its source")
        return 0
    IMAGE.write_bytes(image)
    print(f"wrote {IMAGE} ({len(image)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
