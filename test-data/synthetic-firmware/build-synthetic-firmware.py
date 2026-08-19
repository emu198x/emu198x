#!/usr/bin/env python3
"""Build synthetic, fully-free firmware images that prove a machine boots.

Most machines here need their manufacturer's ROM to reach a prompt, and that
ROM cannot be copied to a public CI runner. So the claim that matters most —
"does this machine start at all" — is the one public CI can least often
check.

A synthetic firmware image answers a weaker but real version of it. Every
machine crate takes its ROM as bytes, so the ROM socket can hold code this
project wrote. That proves the reset vector is fetched, the ROM is readable
where the memory map says it is, the CPU executes, the video chip takes its
programming, and a frame reaches the framebuffer.

**It does not prove the manufacturer's firmware boots.** That is a different
and stronger claim, available only where the real ROM may be distributed —
the Sinclair and Amstrad line, which boots its own firmware in CI. The two
should be worded differently wherever they are reported.

## Why one script covers six machines

The MSX, Memotech MTX, Sord M5, Spectravideo SVI-328, ColecoVision and
Tatung Einstein all drive a TMS9918. The program is therefore the same
everywhere: point the backdrop at a colour the power-on state does not
hold, enable the display, and spin. Only the I/O ports differ, and the
TMS9918's palette is fixed in silicon, so no colour memory needs loading.

Entry 15 is white; entry 0, the power-on value, is transparent. A machine
that never ran the firmware cannot show white.
"""

from __future__ import annotations

import argparse
from pathlib import Path

BACKDROP_INDEX = 0x0F  # TMS9918 palette entry 15 — white.
EXPECTED_ARGB = 0xFFFFFFFF

# Machine -> (image name, ROM size, VDP control port).
#
# Only the control port is needed: registers are written there, and the
# fixed palette means nothing goes to the data port.
MACHINES = {
    "msx": ("msx.rom", 0x8000, 0x99),
    "memotech-mtx": ("memotech-mtx.rom", 0x4000, 0x02),
    "sord-m5": ("sord-m5.rom", 0x2000, 0x11),
    "spectravideo-svi-328": ("spectravideo-svi-328.rom", 0x8000, 0x81),
    "coleco-colecovision": ("coleco-colecovision.rom", 0x2000, 0xBF),
    "tatung-einstein": ("tatung-einstein.rom", 0x2000, 0x09),
}


def vdp_register(control_port: int, index: int, value: int) -> bytes:
    """Write `value` to TMS9918 register `index`.

    The control port takes the data byte, then `$80 | index`.
    """
    return bytes(
        [
            0x3E, value,               # ld a, value
            0xD3, control_port,        # out (port), a
            0x3E, 0x80 | index,        # ld a, $80|index
            0xD3, control_port,        # out (port), a
        ]
    )


def program(control_port: int) -> bytes:
    return b"".join(
        [
            bytes([0xF3]),                                   # di
            vdp_register(control_port, 7, BACKDROP_INDEX),   # backdrop
            vdp_register(control_port, 1, 0x40),             # display on
            vdp_register(control_port, 0, 0x00),             # graphics I
            bytes([0x18, 0xFE]),                             # jr $
        ]
    )


def build(size: int, control_port: int) -> bytes:
    rom = bytearray(b"\xFF" * size)
    code = program(control_port)
    rom[: len(code)] = code
    return bytes(rom)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    for machine, (name, size, port) in sorted(MACHINES.items()):
        path = args.out_dir / name
        path.write_bytes(build(size, port))
        print(f"wrote {path} ({size} bytes, VDP control port ${port:02X})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
