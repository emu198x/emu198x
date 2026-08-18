#!/usr/bin/env python3
"""Build synthetic, fully-free Nintendo cartridges that prove a machine boots.

The NES and the Game Boy declare no firmware in their profiles: a cartridge
is all they need to run. So "does it boot" can be answered with no
commercial ROM — but only with a cartridge the project is free to ship.
This builds one for each.

Companion to `test-data/sega/synthetic-cart/` and to the synthetic C64
KERNAL, and generated for the same reason: the claim that matters most is
the one public CI can least often check, because booting normally needs
firmware nobody may redistribute.

## What each cartridge proves

The smallest program that cannot pass by accident. Each sets the machine's
backdrop to a colour its power-on state does not hold, enables the
display, and spins. A test then asserts the whole screen is that colour.

That single assertion covers the reset vector being fetched from the right
address, instructions executing, memory-mapped registers decoding, the
video chip taking its writes, and a frame reaching the framebuffer. A
machine that hangs or never fetches shows its power-on colour instead.
"""

from __future__ import annotations

import argparse
from pathlib import Path

# ---------------------------------------------------------------------------
# NES
# ---------------------------------------------------------------------------

NES_PRG_BANKS = 1  # 16 KB, NROM-128: mapped at $C000 and mirrored to $8000
NES_CHR_BANKS = 1  # 8 KB of zeros — a blank pattern table renders colour 0
NES_PRG_SIZE = NES_PRG_BANKS * 16 * 1024
NES_PRG_ORIGIN = 0xC000

# Palette index the cartridge writes to $3F00. The PPU powers up with $09
# there, so this cannot be mistaken for an unrun machine.
NES_BACKDROP_INDEX = 0x30
NES_EXPECTED_ARGB = 0xFFFFFEFF


def nes_program() -> bytes:
    """6502: wait for the PPU, point PPUADDR at the backdrop, write a colour.

    The two `bit $2002` loops are not ceremony. A real PPU ignores writes to
    PPUCTRL, PPUMASK, PPUSCROLL and PPUADDR for about 30,000 cycles after
    reset, and this emulator models that. A cartridge that writes its
    palette immediately is silently ignored and paints the power-on colour —
    which is exactly what the first draft of this program did, and how the
    lock got noticed.
    """
    code = bytearray()
    code += bytes([0x78])  # sei
    code += bytes([0xD8])  # cld
    code += bytes([0xA2, 0xFF])  # ldx #$FF
    code += bytes([0x9A])  # txs
    # Two vblanks, the interval real cartridges wait before trusting the PPU.
    for _ in range(2):
        code += bytes([0x2C, 0x02, 0x20])  # bit $2002
        code += bytes([0x10, 0xFB])  # bpl -5
    # PPUADDR ($2006) takes the high byte then the low byte of $3F00.
    code += bytes([0xA9, 0x3F])  # lda #$3F
    code += bytes([0x8D, 0x06, 0x20])  # sta $2006
    code += bytes([0xA9, 0x00])  # lda #$00
    code += bytes([0x8D, 0x06, 0x20])  # sta $2006
    # PPUDATA ($2007) writes the palette entry.
    code += bytes([0xA9, NES_BACKDROP_INDEX])  # lda #index
    code += bytes([0x8D, 0x07, 0x20])  # sta $2007
    # PPUMASK ($2001): show background, including the leftmost column.
    code += bytes([0xA9, 0x0A])  # lda #$0A
    code += bytes([0x8D, 0x01, 0x20])  # sta $2001
    loop = NES_PRG_ORIGIN + len(code)
    code += bytes([0x4C, loop & 0xFF, loop >> 8])  # jmp loop
    return bytes(code)


def build_nes() -> bytes:
    header = bytearray(16)
    header[0:4] = b"NES\x1a"
    header[4] = NES_PRG_BANKS
    header[5] = NES_CHR_BANKS
    header[6] = 0x00  # mapper 0, horizontal mirroring

    prg = bytearray(b"\xFF" * NES_PRG_SIZE)
    code = nes_program()
    prg[0 : len(code)] = code
    # Vectors live at the top of the bank: NMI, RESET, IRQ.
    prg[0x3FFA:0x3FFC] = (NES_PRG_ORIGIN).to_bytes(2, "little")
    prg[0x3FFC:0x3FFE] = (NES_PRG_ORIGIN).to_bytes(2, "little")
    prg[0x3FFE:0x4000] = (NES_PRG_ORIGIN).to_bytes(2, "little")

    chr_rom = bytes(NES_CHR_BANKS * 8 * 1024)
    return bytes(header) + bytes(prg) + chr_rom


# ---------------------------------------------------------------------------
# Game Boy
# ---------------------------------------------------------------------------

GB_ROM_SIZE = 32 * 1024
GB_ENTRY = 0x0150  # just past the header

# The cartridge loads a palette whose colour 0 is the darkest shade. The
# power-on palette register is zero, which is the lightest, so a machine
# that never ran shows white where this shows black.
GB_BGP = 0xFF  # every index -> shade 3


def gb_program() -> bytes:
    """SM83: turn the LCD off, set the palette, turn it back on, spin."""
    code = bytearray()
    code += bytes([0xF3])  # di
    code += bytes([0x31, 0xFE, 0xFF])  # ld sp, $FFFE
    # LCDC ($FF40) = 0: the palette and LCD enable must not be changed
    # mid-frame, and the hardware only guarantees that with the LCD off.
    code += bytes([0xAF])  # xor a
    code += bytes([0xE0, 0x40])  # ldh ($40), a
    # BGP ($FF47) = $FF: every background index maps to the darkest shade.
    code += bytes([0x3E, GB_BGP])  # ld a, $FF
    code += bytes([0xE0, 0x47])  # ldh ($47), a
    # LCDC = $91: LCD on, background on, tile data at $8000.
    code += bytes([0x3E, 0x91])  # ld a, $91
    code += bytes([0xE0, 0x40])  # ldh ($40), a
    code += bytes([0x18, 0xFE])  # jr $
    return bytes(code)


def build_game_boy() -> bytes:
    rom = bytearray(b"\x00" * GB_ROM_SIZE)
    # $0100: the entry point the boot ROM jumps to.
    rom[0x0100:0x0104] = bytes([0x00, 0xC3, GB_ENTRY & 0xFF, GB_ENTRY >> 8])
    code = gb_program()
    rom[GB_ENTRY : GB_ENTRY + len(code)] = code

    rom[0x0134:0x0143] = b"SYNTHETIC BOOT\x00"
    rom[0x0147] = 0x00  # ROM only
    rom[0x0148] = 0x00  # 32 KB
    rom[0x0149] = 0x00  # no RAM

    # Header checksum over $0134-$014C. A real boot ROM refuses to start a
    # cartridge that fails it, so it is set even where ours does not check.
    checksum = 0
    for byte in rom[0x0134:0x014D]:
        checksum = (checksum - byte - 1) & 0xFF
    rom[0x014D] = checksum
    return bytes(rom)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()

    for name, image in (("nes.nes", build_nes()), ("game-boy.gb", build_game_boy())):
        path = args.out_dir / name
        path.write_bytes(image)
        print(f"wrote {path} ({len(image)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
