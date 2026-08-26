#!/usr/bin/env python3
"""Generate a synthetic, fully-free 8 KB C64 KERNAL image for the Lorenz suite.

This is NOT the Commodore/Cloanto KERNAL and contains none of its code. It is
an original 8 KB image whose only meaningful bytes are minimal interrupt
handlers at the three addresses the Lorenz test harness actually routes through;
everything else is RTS filler.

Why this works: the harness (crates/emu198x-mos-6502/tests/lorenz_tests.rs) loads a
KERNAL into $E000-$FFFF but then overwrites the reset/IRQ vectors, installs its
own IRQ stub at $FF48, and traps CHROUT/GETIN/success/fail by address. So the
suite never executes real KERNAL routines except the interrupt handlers reached
via $EA31 (IRQ), $FE66 (BRK), and the NMI vector. Supplying compatible minimal
handlers there reproduces the real-KERNAL result on all 265 cases (verified
2026-07-04: 250 pass / 15 hardware-dependent skips, identical to the real ROM).

Usage: python3 build-synthetic-kernal.py kernal.rom
"""
import sys


def build() -> bytes:
    k = bytearray([0x60] * 0x2000)  # $E000-$FFFF filler = RTS

    def put(addr: int, data: list[int]) -> None:
        o = addr - 0xE000
        k[o:o + len(data)] = bytes(data)

    # IRQ handler ($EA31): the harness's $FF48 stub already pushed A, X, Y —
    # restore them and return (PLA TAY PLA TAX PLA RTI), the standard $EA81 tail.
    put(0xEA31, [0x68, 0xA8, 0x68, 0xAA, 0x68, 0x40])
    # BRK handler ($FE66): same restore+RTI (the stub pushes A, X, Y on the BRK
    # path too before JMP ($0316)).
    put(0xFE66, [0x68, 0xA8, 0x68, 0xAA, 0x68, 0x40])
    # NMI handler ($FE43): hardware pushed only P/PC, so a bare RTI.
    put(0xFE43, [0x40])
    # NMI hardware vector ($FFFA/$FFFB) -> $FE43. The reset/IRQ vectors are
    # overwritten by the harness, so their values here don't matter.
    put(0xFFFA, [0x43, 0xFE])
    return bytes(k)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "kernal.rom"
    data = build()
    with open(out, "wb") as f:
        f.write(data)
    print(f"wrote {len(data)} bytes -> {out}")
