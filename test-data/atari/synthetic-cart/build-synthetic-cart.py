#!/usr/bin/env python3
"""Build synthetic, fully-free Atari cartridges that prove the machines boot.

The 2600 and 7800 declare no firmware: a cartridge is all either needs. So "does it
boot" can be answered with no commercial ROM, given a cartridge the project
is free to ship.

Companion to `test-data/sega/synthetic-cart/` and
`test-data/nintendo/synthetic-cart/`.

## Why this one is different

The other machines have a framebuffer the video chip fills on its own. The
2600 has no framebuffer at all — the TIA paints whatever its colour
registers hold as the beam sweeps, and the picture exists only because the
CPU and the beam run together. Writing `COLUBK` and spinning is therefore a
complete program: every subsequent scanline is painted by hardware that
keeps running.

It also means the picture is not uniform. The TIA renders the 68-clock
horizontal blank as black on every line, so a test has to crop to the
visible window rather than assert the whole buffer.
"""

from __future__ import annotations

import argparse
from pathlib import Path

ROM_SIZE = 4 * 1024
ORIGIN = 0xF000

# The 7800 pads anything up to 16 KB and maps it flat at $C000.
ROM_SIZE_7800 = 16 * 1024
ORIGIN_7800 = 0xC000

# MARIA's BACKGRND register. It shares the TIA's colour encoding and the
# same palette indexed by value >> 1, so both machines paint the same
# shade — which is convenient, because it is the chips that differ, not
# the claim being made about them.
BACKGRND = 0x20

# TIA colour registers hold (hue << 4) | luminance, and the palette is
# indexed by the value >> 1. $1C selects NTSC entry $0E — a bright shade
# nothing near black could be confused with. The TIA powers up at zero.
COLUBK_VALUE = 0x1C
EXPECTED_ARGB = 0xFFD4D478

VSYNC = 0x00
VBLANK = 0x01
COLUBK = 0x09


def program() -> bytes:
    code = bytearray()
    code += bytes([0x78])  # sei
    code += bytes([0xD8])  # cld
    code += bytes([0xA2, 0xFF])  # ldx #$FF
    code += bytes([0x9A])  # txs
    code += bytes([0xA9, 0x00])  # lda #$00
    code += bytes([0x85, VSYNC])  # sta VSYNC — not in sync
    code += bytes([0x85, VBLANK])  # sta VBLANK — output enabled
    code += bytes([0xA9, COLUBK_VALUE])  # lda #$1C
    code += bytes([0x85, COLUBK])  # sta COLUBK
    loop = ORIGIN + len(code)
    code += bytes([0x4C, loop & 0xFF, loop >> 8])  # jmp loop
    return bytes(code)


def program_7800() -> bytes:
    """6502: write MARIA's background colour and spin.

    No display list. With DMA off — the power-on state — MARIA fills every
    line with BACKGRND, so the background register alone is a complete
    picture. A display list would test the DMA engine, which is a
    different claim from "this machine starts".
    """
    code = bytearray()
    code += bytes([0x78])  # sei
    code += bytes([0xD8])  # cld
    code += bytes([0xA2, 0xFF])  # ldx #$FF
    code += bytes([0x9A])  # txs
    code += bytes([0xA9, COLUBK_VALUE])  # lda #$1C
    code += bytes([0x85, BACKGRND])  # sta BACKGRND
    loop = ORIGIN_7800 + len(code)
    code += bytes([0x4C, loop & 0xFF, loop >> 8])  # jmp loop
    return bytes(code)


def build_7800() -> bytes:
    rom = bytearray(b"\xFF" * ROM_SIZE_7800)
    code = program_7800()
    rom[0 : len(code)] = code
    rom[0x3FFA:0x3FFC] = ORIGIN_7800.to_bytes(2, "little")
    rom[0x3FFC:0x3FFE] = ORIGIN_7800.to_bytes(2, "little")
    rom[0x3FFE:0x4000] = ORIGIN_7800.to_bytes(2, "little")
    return bytes(rom)


def build() -> bytes:
    rom = bytearray(b"\xFF" * ROM_SIZE)
    code = program()
    rom[0 : len(code)] = code
    # Vectors at the top of the 4 KB image: NMI, RESET, IRQ.
    rom[0x0FFA:0x0FFC] = ORIGIN.to_bytes(2, "little")
    rom[0x0FFC:0x0FFE] = ORIGIN.to_bytes(2, "little")
    rom[0x0FFE:0x1000] = ORIGIN.to_bytes(2, "little")
    return bytes(rom)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    for name, image in (("atari-2600.a26", build()), ("atari-7800.a78", build_7800())):
        path = args.out_dir / name
        path.write_bytes(image)
        print(f"wrote {path} ({len(image)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
