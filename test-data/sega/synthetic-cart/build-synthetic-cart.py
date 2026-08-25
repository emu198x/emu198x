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
import sys
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

# The SG-1000's TMS9918 has no CRAM at all: the backdrop is an index into a
# palette fixed in silicon, taken from the low nibble of register 7. Entry
# 15 is white, and entry 0 — the power-on value — is transparent, so the
# two cannot be confused.
SG_BACKDROP_INDEX = 0x0F
SG_EXPECTED_ARGB = 0xFFFFFFFF

# The raster cartridge's two backdrop colours, and the line its interrupt is
# aimed at. Green above the split and red below it, so which is which cannot
# be mistaken for a shade of the other.
RASTER_TOP_CRAM = 0x0C  # full green
RASTER_BOTTOM_CRAM = 0x03  # full red
RASTER_SPLIT_R10 = 0x3F  # fire once, 63 lines down


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


def sg_1000_program() -> bytes:
    """The SG-1000's cartridge: shorter, because it has no palette to load.

    Same Z80, same VDP ports, no CRAM. That difference is the reason this
    is a third image rather than the Master System's reused: a machine
    whose backdrop comes from a register cannot be proven by a cartridge
    that writes colour memory.
    """
    return z80(
        bytes([0xF3]),  # di
        vdp_register(7, SG_BACKDROP_INDEX),
        vdp_register(1, 0x40),  # display enable
        vdp_register(0, 0x00),  # graphics I
        bytes([0x18, 0xFE]),  # jr $
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


def raster_split_program() -> bytes:
    """A cartridge that changes the backdrop from a line interrupt.

    Where the boot cartridges answer "did anything happen at all", this one
    answers "when". It paints the screen from the backdrop, arms the line
    counter, and has its interrupt handler switch the backdrop to a second
    colour — so the frame comes out green above a line and red below it, and
    the *position of that edge* is the line interrupt's timing made visible.

    The split is re-armed every frame, so it stands in the same place on every
    one of them and a test can read it from any frame after the first. That
    needs the frame interrupt as well as the line interrupt, and the handler
    tells them apart the way a game does: it reads the status port first and
    looks at bit 7.

    Reading status is also what clears both pending flags, which is why the
    two interrupts must not coincide — with the counter reloading at 63, 127
    and 191, none of them lands on the line the frame flag is set.

    The background is left as powered-up VRAM, all zeros, which in Mode 4 is
    pattern 0 in every cell and colour 0 in every pixel. Colour 0 is
    transparent, so every active pixel shows the backdrop and the split runs
    the full width of the screen.
    """
    body = z80(
        bytes([0xF3]),  # di
        bytes([0x31, 0xF0, 0xDF]),  # ld sp, $DFF0
        bytes([0xED, 0x56]),  # im 1
        # CRAM entries 16 and 17: the backdrop is a sprite-palette index, and
        # the address register walks on by itself between the two writes.
        cram_address(16),
        bytes([0x3E, RASTER_TOP_CRAM]),
        bytes([0xD3, 0xBE]),  # out ($BE), a
        bytes([0x3E, RASTER_BOTTOM_CRAM]),
        bytes([0xD3, 0xBE]),
        vdp_register(7, 0x00),  # backdrop = entry 16
        vdp_register(10, RASTER_SPLIT_R10),  # line counter reload
        vdp_register(1, 0x60),  # display on + frame interrupts
        vdp_register(0, 0x14),  # mode 4 + line interrupts
        bytes([0xFB]),  # ei
        bytes([0x18, 0xFE]),  # jr $
    )
    # The Z80's IM 1 vector is $0038, which the body runs past, so the reset
    # vector jumps over it and the handler sits where the CPU will look. The
    # body then has to start clear of the handler — overlap it and the handler
    # falls through into the setup code instead of returning, which reads as a
    # timing fault rather than a layout one.
    handler_end = 0x38 + len(RASTER_HANDLER)
    entry = 0x0050
    assert entry >= handler_end, f"body at {entry:#06x} would clobber the handler"
    rom = bytearray(b"\xFF" * (entry + len(body)))
    rom[0:3] = bytes([0xC3, entry & 0xFF, entry >> 8])  # jp entry
    rom[0x38:handler_end] = RASTER_HANDLER
    rom[entry : entry + len(body)] = body
    return bytes(rom)


# The handler. It reads the status port first — which is both how a game tells
# a line interrupt from a frame one and what clears the pending flag — then
# turns bit 7 into the backdrop it wants: entry 0 at the top of a frame, entry
# 1 from the split down.
#
# The cost to the write that matters is fixed, and stated here because the
# test computes where the split should land from it: 58 T-states from the
# handler's first instruction to the register write completing, on top of the
# 13 the Z80 spends accepting an interrupt in mode 1.
RASTER_HANDLER = z80(
    bytes([0xDB, 0xBF]),  # in a, ($BF)   11 T  — status, and clears it
    bytes([0x07]),  # rlca                 4 T  — frame flag to bit 0
    bytes([0xE6, 0x01]),  # and $01        7 T
    bytes([0xEE, 0x01]),  # xor $01        7 T  — 0 on a frame, 1 on a line
    bytes([0xD3, 0xBF]),  # out ($BF), a  11 T  — the register's data byte
    bytes([0x3E, 0x87]),  # ld a, $87      7 T  — register 7
    bytes([0xD3, 0xBF]),  # out ($BF), a  11 T  — the backdrop changes here
    bytes([0xFB]),  # ei
    bytes([0xED, 0x4D]),  # reti
)

# What the handler costs before the backdrop changes, and what the Z80 spends
# getting there. A test turns these into a screen position.
HANDLER_T_STATES_TO_WRITE = 11 + 4 + 7 + 7 + 11 + 7 + 11
INTERRUPT_ACCEPTANCE_T_STATES = 13


def build(code: bytes) -> bytes:
    rom = bytearray(b"\xFF" * CART_SIZE)
    rom[0 : len(code)] = code
    return bytes(rom)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    parser.add_argument(
        "--check",
        action="store_true",
        help="rebuild and compare instead of writing",
    )
    args = parser.parse_args()

    images = (
        ("master-system.sms", program(game_gear=False)),
        ("game-gear.gg", program(game_gear=True)),
        ("sg-1000.sg", sg_1000_program()),
        ("master-system-raster.sms", raster_split_program()),
    )
    drifted = []
    for name, code in images:
        path = args.out_dir / name
        image = build(code)
        if args.check:
            if (path.read_bytes() if path.exists() else b"") != image:
                drifted.append(name)
        else:
            path.write_bytes(image)
            print(f"wrote {path} ({CART_SIZE} bytes)")

    if drifted:
        print(f"cartridges have drifted: {', '.join(drifted)}", file=sys.stderr)
        print("Run `python3 build-synthetic-cart.py` and commit.", file=sys.stderr)
        return 1
    if args.check:
        print(f"cartridges are current ({len(images)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
