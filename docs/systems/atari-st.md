# Atari ST

## Status: Not started

Depends on the 68000 CPU core (built for Amiga). The Atari ST has simpler custom hardware than the Amiga — a Shifter chip for video and a YM2149 (AY-compatible) for sound.

## Hardware overview

- **CPU:** Motorola 68000 @ 8 MHz
- **Clock:** 32 MHz master, 68000 at ÷4 (8 MHz)
- **Video:** Shifter — 3 modes: 320×200 (16 colours), 640×200 (4 colours), 640×400 mono. 512-colour palette. No hardware sprites or scrolling.
- **Audio:** Yamaha YM2149 (AY-3-8910 compatible) — 3 tone + noise + envelope. Same chip family as Spectrum 128K AY.
- **I/O:** MFP 68901 (timers, serial, interrupt controller), ACIA 6850 (keyboard/MIDI), DMA controller
- **Memory:** 512KB–4MB RAM, 192KB/256KB TOS ROM
- **Storage:** 3.5" floppy (720KB standard format), ACSI hard disk

## Work needed

- **68000 CPU** — **Done** (shared with Amiga, `cpu-m68k`)
- **Shifter** — bitplane-to-pixel conversion (simpler than Amiga Denise, no copper/blitter)
- **GLUE** — address decoding, bus arbitration, DMA
- **YM2149** — largely reusable from AY-3-8912 (Spectrum 128K) with minor differences
- **MFP 68901** — interrupt controller and timers (essential for ST software)
- **ACIA 6850** — keyboard and MIDI communication
- **TOS ROM** — system firmware, GEM desktop
- **Floppy** — standard PC-compatible format (unlike Amiga's custom MFM)

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `cpu-m68k` | Shared with Amiga | Done |
| `machine-atari-st` | ST machine wiring |
| `emu198x-atari-st` | GUI shell |

## ROMs needed

| File | Size | Description |
|------|------|-------------|
| `tos102.rom` | 192KB | TOS 1.02 (most compatible) |
| `tos104.rom` | 192KB | TOS 1.04 (Rainbow TOS) |
| `tos206.rom` | 256KB | TOS 2.06 (STE) |
