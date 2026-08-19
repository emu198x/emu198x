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


# ---------------------------------------------------------------------------
# Machines with no shared video chip
# ---------------------------------------------------------------------------
#
# The six above share a TMS9918, so one program serves them all. These do
# not share anything: each drives its own display hardware and needs its
# own few instructions, on its own CPU. What they have in common is only
# the shape of the proof — write the one register that floods the display
# with a colour the power-on state cannot produce, then spin.
#
# Each is paired below with a control image that spins immediately without
# touching any hardware. The control is what "the firmware never ran" looks
# like, and every one of them renders black. That is the comparison the
# expected colour is chosen against; it is not guesswork about what a hung
# machine would show.


def vic20_kernal() -> bytes:
    """VIC-20 KERNAL socket ($E000-$FFFF).

    `$900F` carries screen colour, video polarity and border colour in one
    byte, so a single store floods both. `$2A` is screen 2 / normal video /
    border 2 — red on both.

    The BASIC and character ROMs are supplied zero-filled. A blank character
    generator matters: every glyph the VIC fetches is then all-background,
    so the text area cannot speckle the frame with whatever uninitialised
    screen RAM happens to hold.
    """
    rom = bytearray(b"\xFF" * 0x2000)
    rom[:9] = bytes([
        0x78,                     # sei
        0xA9, 0x2A,               # lda #$2A
        0x8D, 0x0F, 0x90,         # sta $900F
        0x4C, 0x06, 0xE0,         # jmp $E006
    ])
    rom[0x1FFC:0x1FFE] = bytes([0x00, 0xE0])   # reset vector -> $E000
    return bytes(rom)


def atari5200_bios() -> bytes:
    """Atari 5200 BIOS socket ($F800-$FFFF).

    ANTIC's DMA is off at power-on, so with no display list fetched the
    whole raster shows GTIA's background register, `COLBK` at `$C01A`. That
    makes this the only one of the three whose frame comes out entirely
    uniform.
    """
    rom = bytearray(b"\xFF" * 0x800)
    rom[:9] = bytes([
        0x78,                     # sei
        0xA9, 0x44,               # lda #$44
        0x8D, 0x1A, 0xC0,         # sta $C01A   COLBK
        0x4C, 0x06, 0xF8,         # jmp $F806
    ])
    rom[0x7FC:0x7FE] = bytes([0x00, 0xF8])     # reset vector -> $F800
    return bytes(rom)


def amiga_kickstart() -> bytes:
    """Amiga Kickstart socket, 512 KiB at $F80000.

    The 68000 takes its initial stack pointer from the first longword and
    its entry point from the second; at reset OVL maps the ROM at $0 so
    both are read from here. The first longword is the 512 KiB Kickstart
    identifier a real ROM carries, which is why it is not a plausible stack
    pointer -- the firmware sets its own stack before that matters.

    With DMA off there are no bitplanes, so the display is `COLOR00` at
    `$DFF180` from edge to edge.
    """
    rom = bytearray(b"\xFF" * 0x8_0000)
    rom[0:4] = bytes([0x11, 0x14, 0x4E, 0xF9])      # 512 KiB Kickstart id
    rom[4:8] = bytes([0x00, 0xF8, 0x00, 0x08])      # entry -> $F80008
    rom[8:18] = bytes([
        0x33, 0xFC, 0x0F, 0x00, 0x00, 0xDF, 0xF1, 0x80,   # move.w #$0F00,$DFF180
        0x60, 0xFE,                                        # bra.s *
    ])
    return bytes(rom)


def vic20_kernal_control() -> bytes:
    """The same socket, spinning immediately. Renders black."""
    rom = bytearray(b"\xFF" * 0x2000)
    rom[:4] = bytes([0x78, 0x4C, 0x01, 0xE0])   # sei; jmp $E001
    rom[0x1FFC:0x1FFE] = bytes([0x00, 0xE0])
    return bytes(rom)


def atari5200_bios_control() -> bytes:
    rom = bytearray(b"\xFF" * 0x800)
    rom[:4] = bytes([0x78, 0x4C, 0x01, 0xF8])   # sei; jmp $F801
    rom[0x7FC:0x7FE] = bytes([0x00, 0xF8])
    return bytes(rom)


def amiga_kickstart_control() -> bytes:
    rom = bytearray(b"\xFF" * 0x8_0000)
    rom[0:4] = bytes([0x11, 0x14, 0x4E, 0xF9])
    rom[4:8] = bytes([0x00, 0xF8, 0x00, 0x08])
    rom[8:10] = bytes([0x60, 0xFE])             # bra.s *
    return bytes(rom)


def acorn_atom() -> bytes:
    """Acorn Atom, 24 KiB ROM. `$D000-$FFFF` maps to `rom[0x3000..]`, so the
    reset vector at `$FFFC` lives at `rom[0x5FFC]` — not where a flat
    16 KiB image would put it.

    The 6847 needs no register write here. `$BF` is a semigraphics-4 cell:
    bit 7 set, colour 3, all four blocks lit. Filling the text screen at
    `$8000` with it turns the whole display area to one colour without
    touching a mode pin.
    """
    rom = bytearray(b"\xFF" * 0x6000)
    rom[0x3000:0x3000 + 17] = bytes([
        0x78,                     # sei
        0xA9, 0xBF,               # lda #$BF
        0xA2, 0x00,               # ldx #$00
        0x9D, 0x00, 0x80,         # sta $8000,x     loop
        0x9D, 0x00, 0x81,         # sta $8100,x
        0xE8,                     # inx
        0xD0, 0xF7,               # bne loop
        0x4C, 0x0E, 0xD0,         # jmp $D00E
    ])
    rom[0x5FFC:0x5FFE] = bytes([0x00, 0xD0])
    return bytes(rom)


def acorn_atom_control() -> bytes:
    rom = bytearray(b"\xFF" * 0x6000)
    rom[0x3000:0x3004] = bytes([0x78, 0x4C, 0x01, 0xD0])
    rom[0x5FFC:0x5FFE] = bytes([0x00, 0xD0])
    return bytes(rom)


def oric_atmos() -> bytes:
    """Oric Atmos, 16 KiB ROM at `$C000`.

    The Oric has no background register: colour lives in the text stream
    itself, as attribute bytes. Filling video RAM at `$BB80` with a paper
    attribute makes every cell background, so the whole frame becomes one
    colour. `$16` is attribute 22 — paper 6.
    """
    rom = bytearray(b"\xFF" * 0x4000)
    rom[:26] = bytes([
        0x78,                     # sei
        0xA9, 0x16,               # lda #$16        paper attribute
        0xA2, 0x00,               # ldx #$00
        0x9D, 0x80, 0xBB,         # sta $BB80,x     loop
        0x9D, 0x80, 0xBC,         # sta $BC80,x
        0x9D, 0x80, 0xBD,         # sta $BD80,x
        0x9D, 0x80, 0xBE,         # sta $BE80,x
        0x9D, 0x80, 0xBF,         # sta $BF80,x
        0xE8,                     # inx
        0xD0, 0xEE,               # bne loop
        0x4C, 0x17, 0xC0,         # jmp $C017
    ])
    rom[0x3FFC:0x3FFE] = bytes([0x00, 0xC0])
    return bytes(rom)


def oric_atmos_control() -> bytes:
    rom = bytearray(b"\xFF" * 0x4000)
    rom[:4] = bytes([0x78, 0x4C, 0x01, 0xC0])
    rom[0x3FFC:0x3FFE] = bytes([0x00, 0xC0])
    return bytes(rom)


def mattel_aquarius() -> bytes:
    """Mattel Aquarius, 8 KiB ROM at `$0000`.

    Screen RAM is at `$3000` and colour RAM at `$3400`, a byte each per
    cell. Spaces everywhere plus one colour attribute everywhere makes the
    whole text area a single background colour.
    """
    return _z80_rom(bytes([
        0xF3,                     # di
        0x21, 0x00, 0x30,         # ld hl,$3000
        0x11, 0x00, 0x04,         # ld de,$0400
        0x36, 0x20,               # l1: ld (hl),$20   space
        0x23, 0x1B, 0x7A, 0xB3,   #     inc hl ; dec de ; ld a,d ; or e
        0x20, 0xF8,               #     jr nz,l1
        0x21, 0x00, 0x34,         # ld hl,$3400
        0x11, 0x00, 0x04,         # ld de,$0400
        0x36, 0x66,               # l2: ld (hl),$66   colour attribute
        0x23, 0x1B, 0x7A, 0xB3,
        0x20, 0xF8,               #     jr nz,l2
        0x18, 0xFE,               # jr $
    ]))


def jupiter_ace() -> bytes:
    """Jupiter Ace, 8 KiB ROM at `$0000`.

    The Ace's character set lives in RAM at `$2C00`, not ROM. So there is
    nothing to fill in video RAM at all: redefining glyph 0 as eight solid
    bytes floods the screen, because power-on video RAM already holds glyph
    0 everywhere. Eight stores, where every other machine here loops over a
    whole screen.

    The Ace is monochrome, so the flood is ink rather than colour.
    """
    return _z80_rom(bytes([
        0xF3,                     # di
        0x21, 0x00, 0x2C,         # ld hl,$2C00      glyph 0
        0x06, 0x08,               # ld b,8
        0x36, 0xFF,               # l1: ld (hl),$FF
        0x23,                     # inc hl
        0x10, 0xFB,               # djnz l1
        0x18, 0xFE,               # jr $
    ]))


# The two machines that need their CRTC programmed first
# ---------------------------------------------------------------------------
#
# Every machine above has *something* on screen at power-on, so one register
# write or one screen fill is enough. The CPC and the BBC have a 6845 whose
# registers are all zero at reset, which means no display exists at all until
# the firmware builds one. Their programs are an order of magnitude longer
# than the rest — around 170 bytes against a dozen — and almost all of that
# is CRTC setup.
#
# The register values are an ordinary text-mode screen for each machine. They
# are not tuned: any values producing a raster would serve, because the claim
# is that the machine runs and paints, not that it paints correctly.

CPC_CRTC = [
    (0, 63), (1, 40), (2, 46), (3, 0x8E), (4, 38), (5, 0),
    (6, 25), (7, 30), (8, 0), (9, 7), (12, 0x30), (13, 0x00),
]

BBC_CRTC = [
    (0, 127), (1, 80), (2, 98), (3, 0x28), (4, 38), (5, 0), (6, 32),
    (7, 34), (8, 0), (9, 7), (10, 0x20), (11, 8), (12, 0x06), (13, 0x00),
]


def amstrad_cpc() -> bytes:
    """Amstrad CPC, 32 KiB firmware — the lower 16 KiB ROM sits at `$0000`.

    The 6845 is addressed through port `$BCxx` (register select) and `$BDxx`
    (data), the Gate Array through `$7Fxx`. `OUT (C),A` is used throughout
    rather than `OUT (n),A`: the latter puts the accumulator on the high
    address byte and would send the port number as the data.

    Power-on RAM is zero, so every pixel is pen 0. Setting pen 0 and the
    border to the same colour floods the frame without touching video RAM.
    """
    code = bytearray([0xF3, 0x0E, 0x00])                     # di ; ld c,0
    for reg, val in CPC_CRTC:
        code += bytes([0x06, 0xBC, 0x3E, reg, 0xED, 0x79])   # select
        code += bytes([0x06, 0xBD, 0x3E, val, 0xED, 0x79])   # data
    code += bytes([0x06, 0x7F])                              # ld b,$7F
    for value in (0x00, 0x46, 0x10, 0x46, 0x80):
        # pen 0, its colour, the border, its colour, then mode/ROM select.
        code += bytes([0x3E, value, 0xED, 0x79])
    code += bytes([0x18, 0xFE])                              # jr $
    return _z80_rom(bytes(code), 0x8000)


def acorn_bbc_micro() -> bytes:
    """BBC Micro, 16 KiB MOS ROM at `$C000`.

    The 6845 is at `$FE00`/`$FE01`, the Video ULA's control register at
    `$FE20` and its palette at `$FE21`.

    The screen is never filled. Every one of the sixteen logical colours is
    mapped to the same physical colour instead, so whatever uninitialised
    screen RAM holds, the display shows one colour. A palette write is
    `(logical << 4) | (physical EOR 7)`.
    """
    physical = 1
    code = bytearray([0x78])                                 # sei
    for reg, val in BBC_CRTC:
        code += bytes([0xA2, reg, 0x8E, 0x00, 0xFE])         # ldx #reg ; stx $FE00
        code += bytes([0xA9, val, 0x8D, 0x01, 0xFE])         # lda #val ; sta $FE01
    code += bytes([0xA9, 0x9C, 0x8D, 0x20, 0xFE])            # lda #$9C ; sta $FE20
    code += bytes([0xA9, (physical ^ 7) & 0x0F])             # lda #(P EOR 7)
    code += bytes([0xA2, 0x10])                              # ldx #16
    loop = len(code)
    code += bytes([0x8D, 0x21, 0xFE])                        # sta $FE21
    code += bytes([0x18, 0x69, 0x10])                        # clc ; adc #$10
    code += bytes([0xCA])                                    # dex
    code += bytes([0xD0, (loop - (len(code) + 2)) & 0xFF])   # bne loop
    here = len(code)
    code += bytes([0x4C, (0xC000 + here) & 0xFF, (0xC000 + here) >> 8])
    return _6502_rom(bytes(code), 0x4000, 0xC000)


def dragon_32() -> bytes:
    """Dragon 32, 16 KiB ROM at `$C000`, MC6809.

    Two things about this machine cost several attempts, and both are
    recorded here because neither is guessable from the outside.

    **The VDG is in a graphics mode at reset, not a text mode.** The SAM
    display base is `$0000` (real firmware moves it to `$0400`), and the mode
    selected by PIA1's port B needs 6144 bytes, not the 512 a 32x16 text
    screen would. Filling 512 bytes fills an eighth of the display and looks
    exactly like a broken renderer.

    **The fill loop needs time.** 6144 iterations of `sta ,x+` outruns ten
    frames, so a run that is too short leaves the screen part-filled — which
    also looks like a broken renderer. The test runs 200 frames; the fill
    completes by about 120.
    """
    code = bytes([
        0x1A, 0x50,              # orcc #$50     mask interrupts
        0x86, 0xFF,              # lda #$FF      every pixel set
        0x8E, 0x00, 0x00,        # ldx #$0000    SAM display base at reset
        0xA7, 0x80,              # loop: sta ,x+
        0x8C, 0x18, 0x00,        #       cmpx #$1800   6144 bytes
        0x26, 0xF9,              #       bne loop
        0x20, 0xFE,              # bra $
    ])
    rom = bytearray(b"\xFF" * 0x4000)
    rom[:len(code)] = code
    rom[0x3FFE:0x4000] = bytes([0xC0, 0x00])   # 6809 reset vector is at $FFFE
    return bytes(rom)


def dragon_32_control() -> bytes:
    rom = bytearray(b"\xFF" * 0x4000)
    rom[:4] = bytes([0x1A, 0x50, 0x20, 0xFE])
    rom[0x3FFE:0x4000] = bytes([0xC0, 0x00])
    return bytes(rom)


def acorn_electron() -> bytes:
    """Acorn Electron, 16 KiB OS ROM at `$C000`.

    The screen is never filled, and filling it would prove nothing: the ULA
    powers on with all eight palette registers at `$FF`, which decodes to
    **all sixteen logical colours white**. Attempts filling screen RAM with
    `$FF` and then `$00` both left the frame 100% white, because screen
    content was invisible rather than unwritten.

    So this writes the palette instead. The ULA stores each register
    inverted (`written EOR $FF`), and the logical-to-physical mapping is
    scrambled across register *pairs* — red, green and blue for one colour
    come from individual, non-contiguous bits of two registers. Working
    backwards through that decode, every logical colour is red when the even
    registers store `$00` and the odd ones store `$0F`, which means writing
    `$FF` and `$F0`.
    """
    code = bytearray([0x78])                       # sei
    code += bytes([0xA9, 0xFF])                    # lda #$FF -> stored $00
    for addr in (0x08, 0x0A, 0x0C, 0x0E):
        code += bytes([0x8D, addr, 0xFE])          # sta $FE08/0A/0C/0E
    code += bytes([0xA9, 0xF0])                    # lda #$F0 -> stored $0F
    for addr in (0x09, 0x0B, 0x0D, 0x0F):
        code += bytes([0x8D, addr, 0xFE])
    here = len(code)
    code += bytes([0x4C, (0xC000 + here) & 0xFF, (0xC000 + here) >> 8])
    return _6502_rom(bytes(code), 0x4000, 0xC000)


def acorn_electron_control() -> bytes:
    return _6502_rom(bytes([0x78, 0x4C, 0x01, 0xC0]), 0x4000, 0xC000)


def commodore_pet_kernal() -> bytes:
    """Commodore PET, 4 KiB KERNAL at `$F000`.

    The PET is monochrome, so there is no colour to flood. The signal is
    character output instead: the character ROM beside this one defines
    glyph 0 as blank and glyph 1 as solid, and this fills screen RAM at
    `$8000` with glyph 1.

    That split is the load-bearing part. A character ROM whose *every* glyph
    was solid would light the screen without the CPU doing anything, and the
    test would pass on a machine that never executed an instruction. Power-on
    screen RAM holds glyph 0, which is blank, so the display can only light
    if the fill ran.
    """
    code = bytes([
        0x78,                    # sei
        0xA9, 0x01,              # lda #$01     glyph 1
        0xA2, 0x00,              # ldx #$00
        0x9D, 0x00, 0x80,        # loop: sta $8000,x
        0x9D, 0x00, 0x81,        #       sta $8100,x
        0x9D, 0x00, 0x82,        #       sta $8200,x
        0x9D, 0x00, 0x83,        #       sta $8300,x
        0xE8,                    #       inx
        0xD0, 0xF1,              #       bne loop
        0x4C, 0x14, 0xF0,        # jmp $F014
    ])
    return _6502_rom(code, 0x1000, 0xF000)


def commodore_pet_kernal_control() -> bytes:
    return _6502_rom(bytes([0x78, 0x4C, 0x01, 0xF0]), 0x1000, 0xF000)


def commodore_pet_chargen() -> bytes:
    """Glyph 0 blank, glyph 1 solid, the rest blank. The blank glyph 0 is
    what stops the screen lighting without the CPU."""
    rom = bytearray(0x800)
    rom[8:16] = b"\xFF" * 8
    return bytes(rom)


def _sinclair_zx_rom(size: int, with_program: bool) -> bytes:
    """ZX80 (4 KiB) and ZX81 (8 KiB) — the same image, cut to length.

    ## What this proves here, and what it would prove on real hardware

    On a real ZX80/ZX81 the *CPU* generates the picture: the Z80 executes
    through the display file while the ULA forces NOPs and turns the fetched
    bytes into video. **This emulator does not model that.** Its ULA reads
    the `D_FILE` pointer from the system variables at `$400C` and renders the
    display file directly.

    So this image sets up `D_FILE` and writes a display file, and a pass
    proves the CPU executed and those writes landed. It does **not** prove
    the display-generation mechanism works, because there is nothing here to
    exercise. On a faithful implementation this same image would prove far
    more — or fail.

    ## Why the ROM is laid out the way it is

    The ULA takes character bitmaps from the **first 512 bytes of ROM**,
    which is also where the Z80 must start executing. So the program is not
    at `$0000`: a `jp $0100` sits there instead, leaving `$08-$0F` — the
    bitmap for character 1 — zeroed.

    The display file is then filled with `$81`, character 1 *inverted*, which
    renders solid because inverting a blank bitmap sets every pixel. A
    character that was already solid in ROM would light the screen without
    the firmware running, which is the trap the PET's character ROM avoids
    the same way.
    """
    rom = bytearray(b"\x00" * size)          # zeros: every glyph blank
    rom[0:3] = bytes([0xC3, 0x00, 0x01])      # jp $0100
    if not with_program:
        rom[0x100:0x102] = bytes([0x18, 0xFE])    # jr $ — touch nothing
        return bytes(rom)
    code = bytearray()
    code += bytes([0xF3])                     # di
    code += bytes([0x21, 0x00, 0x41])         # ld hl,$4100
    code += bytes([0x22, 0x0C, 0x40])         # ld ($400C),hl    D_FILE pointer
    code += bytes([0x36, 0x76])               # ld (hl),$76      leading NEWLINE
    code += bytes([0x23])                     # inc hl
    code += bytes([0x06, 24])                 # ld b,24          rows
    row = len(code)
    code += bytes([0x0E, 32])                 # ld c,32          columns
    col = len(code)
    code += bytes([0x36, 0x81])               # ld (hl),$81      inverse char 1
    code += bytes([0x23, 0x0D])               # inc hl ; dec c
    code += bytes([0x20, (col - (len(code) + 2)) & 0xFF])   # jr nz,col
    code += bytes([0x36, 0x76])               # ld (hl),$76      NEWLINE
    code += bytes([0x23])                     # inc hl
    code += bytes([0x10, (row - (len(code) + 2)) & 0xFF])   # djnz row
    code += bytes([0x18, 0xFE])               # jr $
    rom[0x100:0x100 + len(code)] = code
    return bytes(rom)


def sinclair_zx80() -> bytes:
    return _sinclair_zx_rom(0x1000, True)


def sinclair_zx80_control() -> bytes:
    return _sinclair_zx_rom(0x1000, False)


def sinclair_zx81() -> bytes:
    return _sinclair_zx_rom(0x2000, True)


def sinclair_zx81_control() -> bytes:
    return _sinclair_zx_rom(0x2000, False)


def _6502_rom(program: bytes, size: int, base: int) -> bytes:
    """A 6502 ROM with its reset vector pointed at the first byte."""
    rom = bytearray(b"\xFF" * size)
    rom[:len(program)] = program
    vector = size - 4
    rom[vector] = base & 0xFF
    rom[vector + 1] = base >> 8
    return bytes(rom)


def amstrad_cpc_control() -> bytes:
    return _z80_rom(bytes([0xF3, 0x18, 0xFE]), 0x8000)


def acorn_bbc_micro_control() -> bytes:
    return _6502_rom(bytes([0x78, 0x4C, 0x01, 0xC0]), 0x4000, 0xC000)


def _z80_rom(program: bytes, size: int = 0x2000) -> bytes:
    """A Z80 ROM at $0000 — reset lands on the first byte, no vector needed."""
    rom = bytearray(b"\xFF" * size)
    rom[:len(program)] = program
    return bytes(rom)


def spin_only_z80() -> bytes:
    return _z80_rom(bytes([0xF3, 0x18, 0xFE]))


def zero_rom(size: int) -> bytes:
    return b"\x00" * size


# name -> builder. Written flat so adding a machine is one line plus one
# function, and so the file list is greppable.
SINGLE_MACHINE = {
    "commodore-vic-20-kernal.rom": vic20_kernal,
    "commodore-vic-20-basic.rom": lambda: zero_rom(0x2000),
    "commodore-vic-20-chargen.rom": lambda: zero_rom(0x1000),
    "atari-5200-bios.rom": atari5200_bios,
    "commodore-amiga-kickstart.rom": amiga_kickstart,
    # Controls: identical sockets, no hardware touched. Each renders black,
    # which is what makes the expected colour above evidence rather than a
    # value someone liked the look of.
    "sinclair-zx80.rom": sinclair_zx80,
    "sinclair-zx80-control.rom": sinclair_zx80_control,
    "sinclair-zx81.rom": sinclair_zx81,
    "sinclair-zx81-control.rom": sinclair_zx81_control,
    "acorn-electron.rom": acorn_electron,
    "acorn-electron-control.rom": acorn_electron_control,
    "acorn-electron-basic.rom": lambda: zero_rom(0x4000),
    "commodore-pet-kernal.rom": commodore_pet_kernal,
    "commodore-pet-kernal-control.rom": commodore_pet_kernal_control,
    "commodore-pet-chargen.rom": commodore_pet_chargen,
    "commodore-pet-basic.rom": lambda: zero_rom(0x2000),
    "commodore-pet-editor.rom": lambda: zero_rom(0x800),
    "dragon-32.rom": dragon_32,
    "dragon-32-control.rom": dragon_32_control,
    "amstrad-cpc.rom": amstrad_cpc,
    "acorn-bbc-micro.rom": acorn_bbc_micro,
    "amstrad-cpc-control.rom": amstrad_cpc_control,
    "acorn-bbc-micro-control.rom": acorn_bbc_micro_control,
    "mattel-aquarius.rom": mattel_aquarius,
    "jupiter-ace.rom": jupiter_ace,
    "mattel-aquarius-control.rom": spin_only_z80,
    "jupiter-ace-control.rom": spin_only_z80,
    "acorn-atom.rom": acorn_atom,
    "oric-atmos.rom": oric_atmos,
    "acorn-atom-control.rom": acorn_atom_control,
    "oric-atmos-control.rom": oric_atmos_control,
    "commodore-vic-20-kernal-control.rom": vic20_kernal_control,
    "atari-5200-bios-control.rom": atari5200_bios_control,
    "commodore-amiga-kickstart-control.rom": amiga_kickstart_control,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    for machine, (name, size, port) in sorted(MACHINES.items()):
        path = args.out_dir / name
        path.write_bytes(build(size, port))
        print(f"wrote {path} ({size} bytes, VDP control port ${port:02X})")
    for name, builder in sorted(SINGLE_MACHINE.items()):
        path = args.out_dir / name
        image = builder()
        path.write_bytes(image)
        print(f"wrote {path} ({len(image)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
