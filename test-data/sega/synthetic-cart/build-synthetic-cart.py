#!/usr/bin/env python3
"""Build synthetic, fully-free Sega cartridges that prove a machine boots.

The Master System, Game Gear and SG-1000 declare no firmware: a cartridge
is all they need to run. So "does it boot" can be verified with no
commercial ROM anywhere — but only if a cartridge exists that the project
is free to ship. This builds one.

Follows the precedent of the synthetic C64 KERNAL in
`test-data/commodore/c64/synthetic-kernal/`: generated here, explained in
the README beside it, and containing no code from any commercial image.

## What the cartridge proves

It is deliberately the smallest program that cannot pass by accident. On
reset the Z80 fetches from `$0000`, and the program:

1. disables interrupts and sets up the stack,
2. writes a colour into CRAM through port `$BE`,
3. points the backdrop at that colour via VDP register 7,
4. enables the display via register 1,
5. halts.

A test then asserts the framebuffer is filled with that exact colour. The
default CRAM is all zeros, so the colour cannot appear unless the
cartridge actually executed: reset vector fetched, instructions run, I/O
ports decoded, VDP registers written, frame emitted. A machine that hangs,
never fetches, or ignores its ports produces black instead.

The Game Gear takes the same program with a different CRAM write, because
its palette is 12-bit across two bytes where the Master System's is 6-bit
in one. That difference is the point of a separate image: the handheld is
not a Master System with a smaller screen.
"""

from __future__ import annotations

import argparse
from pathlib import Path

# 32 KB is the smallest size the Sega mapper is happy to page.
CART_SIZE = 32 * 1024

# The colour each machine paints. Chosen so a wrong CRAM format cannot
# accidentally produce it: on the Master System, 6-bit %00BBGGRR.
SMS_CRAM_BYTE = 0x0C  # BB=00 GG=11 RR=00 -> full green
SMS_EXPECTED_ARGB = 0xFF00FF00

# Game Gear: 12-bit, low byte %GGGGRRRR, high byte %----BBBB.
GG_CRAM_LO = 0xF0  # G=F, R=0
GG_CRAM_HI = 0x00  # B=0
GG_EXPECTED_ARGB = 0xFF00FF00


def z80(*parts: bytes) -> bytes:
    return b"".join(parts)


def vdp_register(index: int, value: int) -> bytes:
    """Write `value` to VDP register `index` through the control port.

    The control port takes a data byte then a command byte; for a register
    write the command is `$80 | index`.
    """
    return z80(
        bytes([0x3E, value]),  # ld a, value
        bytes([0xD3, 0xBF]),  # out ($BF), a
        bytes([0x3E, 0x80 | index]),  # ld a, $80|index
        bytes([0xD3, 0xBF]),  # out ($BF), a
    )


def cram_address(entry: int) -> bytes:
    """Point the VDP's address register at CRAM `entry`.

    Code `$C0` in the top bits selects CRAM rather than VRAM.
    """
    return z80(
        bytes([0x3E, entry]),  # ld a, entry
        bytes([0xD3, 0xBF]),  # out ($BF), a
        bytes([0x3E, 0xC0]),  # ld a, $C0
        bytes([0xD3, 0xBF]),  # out ($BF), a
    )


def program(game_gear: bool) -> bytes:
    body = z80(
        bytes([0xF3]),  # di
        bytes([0x31, 0xF0, 0xDF]),  # ld sp, $DFF0
        # The backdrop is taken from the sprite palette, which starts at
        # CRAM entry 16. Entry 16 is colour 0 of that palette.
        cram_address(32 if game_gear else 16),
    )
    if game_gear:
        body += z80(
            bytes([0x3E, GG_CRAM_LO]),
            bytes([0xD3, 0xBE]),  # out ($BE), a
            bytes([0x3E, GG_CRAM_HI]),
            bytes([0xD3, 0xBE]),
        )
    else:
        body += z80(
            bytes([0x3E, SMS_CRAM_BYTE]),
            bytes([0xD3, 0xBE]),  # out ($BE), a
        )
    body += z80(
        # Register 7: backdrop colour = sprite-palette entry 0.
        vdp_register(7, 0x00),
        # Register 1: bit 6 enables the display.
        vdp_register(1, 0x40),
        # Register 0: mode 4.
        vdp_register(0, 0x04),
        bytes([0x18, 0xFE]),  # jr $ — halt without needing interrupts
    )
    return body


def build(game_gear: bool) -> bytes:
    rom = bytearray(b"\xFF" * CART_SIZE)
    rom[0 : len(program(game_gear))] = program(game_gear)
    return bytes(rom)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()

    for name, game_gear in (("master-system.sms", False), ("game-gear.gg", True)):
        path = args.out_dir / name
        path.write_bytes(build(game_gear))
        print(f"wrote {path} ({CART_SIZE} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
