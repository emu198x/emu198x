# Sega Mega Drive / Genesis

## Status: Not started

Depends on both the 68000 CPU (built for Amiga) and Z80 CPU (done). The Mega Drive is a dual-CPU system — 68000 for main processing and Z80 for sound.

## Hardware overview

- **Main CPU:** Motorola 68000 @ 7.67 MHz (NTSC) / 7.61 MHz (PAL)
- **Sound CPU:** Zilog Z80 @ 3.58 MHz (already implemented)
- **Clock:** 53.693175 MHz master (NTSC)
- **Video:** VDP (Video Display Processor, Sega custom) — 320×224 (NTSC) or 320×240 (PAL), 2 scrolling planes + window plane, 80 sprites, 61 colours on screen from 512-colour palette, DMA, H/V interrupts
- **Audio:** Yamaha YM2612 FM synthesis (6 operators, 6 channels) + SN76489 PSG (3 tone + 1 noise, from Z80)
- **Memory:** 64KB main RAM (68000), 8KB sound RAM (Z80), 64KB VRAM
- **Storage:** Cartridge (ROM, up to 4MB typical)

## Work needed

- **68000 CPU** — **Done** (shared with Amiga, `cpu-m68k`)
- **Z80 CPU** — **Done** (from Spectrum, `cpu-z80`)
- **VDP** — tile-based with two scroll planes, window, sprites, DMA. Derived from Master System VDP. H-blank and V-blank interrupts essential.
- **YM2612** — 6-channel FM synthesis. Complex but well-documented.
- **SN76489** — simple PSG (same as BBC Micro, Master System)
- **Bus arbitration** — 68000/Z80 shared bus with bank switching window
- **Cartridge format** — ROM header parsing, mappers (SRAM, EEPROM, bank switching for large ROMs)

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `cpu-m68k` | Shared with Amiga | Done |
| `cpu-z80` | Shared with Spectrum | Done |
| `sega-vdp` | Mega Drive VDP |
| `yamaha-ym2612` | FM synthesis |
| `machine-sega-megadrive` | Machine wiring |
| `emu198x-sega-megadrive` | GUI shell |

## ROMs

Cartridge ROMs (with header parsing); no system BIOS required for most carts
(TMSS BIOS optional on later models).

## Known unknowns / disproven hypotheses

- **Open: not started.** Both CPUs are shared and validated; no Mega Drive
  hardware runs yet.
- **Verification targets** — the Sega VDP (two scroll planes + window + 80
  sprites + DMA, "derived from the Master System VDP" — confirm how much
  actually carries over), the YM2612 FM path, and the 68000/Z80 bus-arbitration
  + bank-switch window are from secondary knowledge. Confirm against the Genesis
  Software/Hardware Manuals and `emulators/multi-system/` (ares) before
  implementing.

## Validated against

- 68000 (Tom Harte) + Z80 (Tom Harte/ZEXALL) cores — shared, validated
  elsewhere. Nothing Mega-Drive-specific yet.
