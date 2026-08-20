#!/usr/bin/env python3
"""Build cartridges that show the Emu198x wordmark, for machines that need one.

Five machines in this workspace will not start without media: the NES, Game
Boy, Atari 2600, 5200 and 7800. Everything else either boots its own firmware
or has a synthetic image in `../synthetic-firmware`. So the machines with the
least to prove in public CI are exactly the ones with no way to prove it.

These cartridges close that. They are ours from source, so they carry no
provenance question and can sit in the repository — and they draw through the
real video path rather than poking a framebuffer, so a pass says the CPU ran,
the video chip took its programming, and a frame reached the screen.

The wordmark is set in each machine's own tiles rather than converted from
artwork. That keeps each cartridge small and idiomatic, and it means a logo
redesign does not invalidate them.

Assembly uses this project's own assembler. Run with no arguments to write
every image; `--check` rebuilds and compares instead, for CI.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def assemble(source: Path, dialect: str) -> bytes:
    """Assembles one source with asm198x and returns the raw bytes."""
    out = source.with_suffix(".tmp.bin")
    try:
        subprocess.run(
            ["asm198x", "asm", "--dialect", dialect, str(source), "-o", str(out)],
            check=True,
            capture_output=True,
            text=True,
        )
        return out.read_bytes()
    finally:
        out.unlink(missing_ok=True)



# --------------------------------------------------------------------------
# The mark
# --------------------------------------------------------------------------
#
# `198x/decisions/family-visual-identity.md` makes the family mark a divider
# plate: two cells with a full-height rule between them, the prefix cell
# filled and the `198x` cell never varying, framed in a constant house brown.
# The **filled** rendering is the one for "where the plate must hold on its
# own", which a boot screen is.
#
# The plate is drawn here as a bitmap and cut into tiles, rather than set as
# text in a tile font. Keeping the picture in the builder and the program in
# the assembly means the artwork can be redrawn without anyone editing Z80 —
# and it means the mark is the mark, not a typographic approximation of it.
#
# The one honest departure is colour. The prefix cell should carry Emu198x's
# `#0d4a7d`; four greys cannot hold it, so it takes the darker mid tone and
# the cell still reads as filled. The colour axis is not faked in a palette
# that cannot carry it.

FONT = {
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "M": ["#...#", "##.##", "#.#.#", "#...#", "#...#", "#...#", "#...#"],
    "U": ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "9": [".###.", "#...#", "#...#", ".####", "....#", "#...#", ".###."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "x": [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
}

# The plate's three roles. Each machine maps them onto its own colour
# indices, so the drawing code never knows what hardware it is for.
PAPER, FILL, INK = 0, 1, 2

SCALE = 2
PLATE_W, PLATE_H = 16, 3  # tiles
DIVIDER_X = 52  # pixels from the plate's left edge


def _text_width(text: str) -> int:
    return sum(len(FONT[c][0]) + 1 for c in text) * SCALE - SCALE


def _draw(px, text, x, y, colour) -> None:
    for ch in text:
        glyph = FONT[ch]
        for row, bits in enumerate(glyph):
            for col, cell in enumerate(bits):
                if cell != "#":
                    continue
                for dy in range(SCALE):
                    for dx in range(SCALE):
                        px[y + row * SCALE + dy][x + col * SCALE + dx] = colour
        x += (len(glyph[0]) + 1) * SCALE


def plate_bitmap(prefix: str = "EMU") -> list[list[int]]:
    """Draws the filled plate as a grid of Game Boy colour indices."""
    width, height = PLATE_W * 8, PLATE_H * 8
    px = [[PAPER] * width for _ in range(height)]

    for x in range(width):
        px[0][x] = px[height - 1][x] = INK
    for y in range(height):
        px[y][0] = px[y][width - 1] = INK
        px[y][DIVIDER_X] = INK
    for y in range(1, height - 1):
        for x in range(1, DIVIDER_X):
            px[y][x] = FILL

    top = (height - 7 * SCALE) // 2
    _draw(px, prefix, 1 + (DIVIDER_X - 1 - _text_width(prefix)) // 2, top, PAPER)
    _draw(
        px,
        "198x",
        DIVIDER_X + 1 + (width - 2 - DIVIDER_X - _text_width("198x")) // 2,
        top,
        INK,
    )
    return px


# Game Boy: four greys, so the filled cell takes the darker mid tone. It
# cannot carry Emu198x's #0d4a7d and the colour axis is not faked in a
# palette that cannot hold it.
GB_COLOUR = {PAPER: 0, FILL: 2, INK: 3}


def _gb_encode(cell) -> list[tuple[int, int]]:
    """Game Boy 2bpp: one byte of low bits, one of high, per row."""
    rows = []
    for row in cell:
        low = high = 0
        for bit, role in enumerate(row):
            value = GB_COLOUR[role]
            if value & 1:
                low |= 0x80 >> bit
            if value & 2:
                high |= 0x80 >> bit
        rows.append((low, high))
    return rows


def gb_art_source() -> str:
    """Cuts the plate into tiles and emits the assembly the program expects."""
    px = plate_bitmap()
    blank = tuple(tuple(PAPER for _ in range(8)) for _ in range(8))
    # Tile 0 is reserved paper: the program clears the map to it, so it has to
    # be blank rather than whichever corner of the picture was cut first.
    tiles, index, plate = [blank], {blank: 0}, []
    for row in range(PLATE_H):
        for col in range(PLATE_W):
            cell = tuple(
                tuple(px[row * 8 + y][col * 8 + x] for x in range(8)) for y in range(8)
            )
            if cell not in index:
                index[cell] = len(tiles)
                tiles.append(cell)
            plate.append(index[cell])

    out = [
        "",
        f"DEF PLATE_W   EQU {PLATE_W}",
        f"DEF PLATE_H   EQU {PLATE_H}",
        f"DEF PLATE_COL EQU {(20 - PLATE_W) // 2}",
        f"DEF PLATE_ROW EQU {(18 - PLATE_H) // 2}",
        "",
        'SECTION "Art", ROM0[$0400]',
        f"; {len(tiles)} unique tiles cut from a {PLATE_W * 8}x{PLATE_H * 8} bitmap.",
        "Tiles:",
    ]
    for i, cell in enumerate(tiles):
        out.append(f"    ; tile {i}")
        out += [f"    db ${lo:02X}, ${hi:02X}" for lo, hi in _gb_encode(cell)]
    out += ["TilesEnd:", "", f"; {PLATE_W}x{PLATE_H} map, row-major.", "Plate:"]
    for row in range(PLATE_H):
        cells = plate[row * PLATE_W : (row + 1) * PLATE_W]
        out.append("    db " + ", ".join(str(v) for v in cells))
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------
# Game Boy
# --------------------------------------------------------------------------

GB_ROM_SIZE = 0x8000  # The smallest legal image: header plus one 32 KiB bank.
GB_CODE_ORIGIN = 0x0100  # Where the assembled section starts, and where the
# CPU begins on a machine with no boot ROM.


def game_boy() -> bytes:
    """A 32 KiB ROM-only cartridge.

    The header is written here rather than in the assembly because it is data
    about the image, not code in it — and because the checksum has to be taken
    over bytes the assembler has already emitted.
    """
    program = (HERE / "nintendo-game-boy-plate.asm").read_text()
    combined = HERE / "nintendo-game-boy-plate.combined.asm"
    combined.write_text(program + gb_art_source())
    try:
        code = assemble(combined, "rgbasm")
    finally:
        combined.unlink(missing_ok=True)
    rom = bytearray(b"\x00" * GB_ROM_SIZE)
    rom[GB_CODE_ORIGIN : GB_CODE_ORIGIN + len(code)] = code

    title = b"EMU198X"
    rom[0x0134 : 0x0134 + len(title)] = title
    rom[0x0147] = 0x00  # ROM only: no mapper, no RAM.
    rom[0x0148] = 0x00  # 32 KiB.
    rom[0x0149] = 0x00  # No cartridge RAM.

    # The header checksum the DMG boot ROM would verify. This core does not
    # check it, so getting it right is a courtesy to every other emulator
    # someone might open the file in.
    checksum = 0
    for byte in rom[0x0134:0x014D]:
        checksum = (checksum - byte - 1) & 0xFF
    rom[0x014D] = checksum

    return bytes(rom)




# --------------------------------------------------------------------------
# NES
# --------------------------------------------------------------------------

# The NES has a palette, so the filled cell carries a colour rather than a
# tone. Emu198x is #0d4a7d, which the hardware does not have.
#
# `$01` (#002a88) is the choice, and not the one a nearest-colour search
# gives. Measured against this core's own palette table, the closest entry is
# `$0C` (#00404d) — but that is a dark teal, and teal at hue 205 is Isa198x's
# colour in the family palette. An identity mark rendered in a sibling's hue
# is worse than one a few degrees off its own, because the whole point of the
# colour axis is telling the siblings apart.
#
# So the rule here is nearest *blue*, not nearest colour. White lettering
# clears it comfortably.
NES_PALETTE = (0x30, 0x01, 0x0F, 0x0F)  # paper, fill, ink, unused
NES_COLOUR = {PAPER: 0, FILL: 1, INK: 2}

NES_NAMETABLE_W, NES_NAMETABLE_H = 32, 30


def _nes_encode(cell) -> bytes:
    """NES 2bpp is planar: eight bytes of low bits, then eight of high."""
    low, high = bytearray(), bytearray()
    for row in cell:
        lo = hi = 0
        for bit, role in enumerate(row):
            value = NES_COLOUR[role]
            if value & 1:
                lo |= 0x80 >> bit
            if value & 2:
                hi |= 0x80 >> bit
        low.append(lo)
        high.append(hi)
    return bytes(low + high)


def _cut(px, blank_role=PAPER):
    """Cuts a bitmap into 8x8 tiles, de-duplicated, tile 0 reserved blank."""
    blank = tuple(tuple(blank_role for _ in range(8)) for _ in range(8))
    tiles, index, layout = [blank], {blank: 0}, []
    rows, cols = len(px) // 8, len(px[0]) // 8
    for row in range(rows):
        for col in range(cols):
            cell = tuple(
                tuple(px[row * 8 + y][col * 8 + x] for x in range(8)) for y in range(8)
            )
            if cell not in index:
                index[cell] = len(tiles)
                tiles.append(cell)
            layout.append(index[cell])
    return tiles, layout


def nes_art_source() -> tuple[str, bytes]:
    """Returns the assembly the program expects, and the 8 KB CHR bank."""
    px = plate_bitmap()
    tiles, layout = _cut(px)

    col = (NES_NAMETABLE_W - PLATE_W) // 2
    row = (NES_NAMETABLE_H - PLATE_H) // 2
    out = [
        "",
        f"PLATE_W = {PLATE_W}",
        f"PLATE_BASE = NAMETABLE + {row} * 32 + {col}",
        "",
        # This assembler links a fixed NROM layout with no RODATA area, so
        # read-only data rides in CODE — which on NROM is the same ROM anyway.
        ".segment \"CODE\"",
        "palette:",
        "    .byte " + ", ".join(f"${v:02X}" for v in NES_PALETTE),
    ]
    for r in range(PLATE_H):
        cells = layout[r * PLATE_W : (r + 1) * PLATE_W]
        out.append(f"PlateRow{r}:")
        out.append("    .byte " + ", ".join(str(v) for v in cells))

    chr_bank = bytearray()
    for cell in tiles:
        chr_bank += _nes_encode(cell)
    chr_bank += b"\x00" * (8192 - len(chr_bank))
    return "\n".join(out) + "\n", bytes(chr_bank)


def nes() -> bytes:
    """An NROM cartridge: 16-byte header, 32 KB PRG, 8 KB CHR.

    asm198x's ca65 dialect links the whole iNES image, so the CHR bank goes
    in through a `CHARS` segment rather than being appended here — the
    assembler owns the layout, and a builder that appended bytes would be
    guessing at it.
    """
    source, chr_bank = nes_art_source()
    chars = "\n.segment \"CHARS\"\n" + "\n".join(
        "    .byte " + ", ".join(f"${b:02X}" for b in chr_bank[i : i + 16])
        for i in range(0, len(chr_bank), 16)
    )
    program = (HERE / "nintendo-nes-plate.s").read_text()
    combined = HERE / "nintendo-nes-plate.combined.s"
    combined.write_text(program + source + chars + "\n")
    try:
        return assemble(combined, "ca65")
    finally:
        combined.unlink(missing_ok=True)


CARTRIDGES = {
    "nintendo-game-boy-logo.gb": game_boy,
    "nintendo-nes-logo.nes": nes,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="rebuild and compare instead of writing",
    )
    args = parser.parse_args()

    drifted = []
    for name, build in CARTRIDGES.items():
        path = HERE / name
        image = build()
        if args.check:
            current = path.read_bytes() if path.exists() else b""
            if current != image:
                drifted.append(name)
        else:
            path.write_bytes(image)
            print(f"wrote {path.name} ({len(image)} bytes)")

    if drifted:
        print(f"cartridges have drifted: {', '.join(drifted)}", file=sys.stderr)
        print("Run `python3 build-synthetic-cartridges.py` and commit.", file=sys.stderr)
        return 1
    if args.check:
        print(f"cartridges are current ({len(CARTRIDGES)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
