# BBC Micro

## Status: Not started

Depends on the 6502 CPU core (built for C64). The BBC Micro has relatively clean hardware with a Motorola 6845 CRTC and versatile video modes.

## Hardware overview

- **CPU:** MOS 6502A @ 2 MHz
- **Clock:** 16 MHz master, CPU at ÷8 (2 MHz) or ÷4 during video access
- **Video:** Motorola 6845 CRTC + SAA5050 Teletext chip — 8 display modes (MODE 0-7), from 640×256 monochrome to 160×256 with 8 colours, plus Teletext MODE 7
- **Audio:** SN76489 — 3 tone channels + 1 noise, 4-bit volume each
- **I/O:** MOS 6522 VIA × 2 — keyboard, sound chip, system timers, user port, printer port
- **Memory:** 32KB RAM (Model B), 16KB ROM (MOS), 16KB sideways ROM/RAM slots
- **Storage:** Cassette (300/1200 baud), floppy (optional, Intel 8271 or WD1770)

## Work needed

- **6502 CPU** — **Done** (shared with C64, `cpu-6502`)
- **6845 CRTC** — cursor, character timing, screen address. BBC adds custom logic for its display modes.
- **SAA5050 Teletext** — MODE 7 character generator
- **SN76489** — 4-channel sound
- **6522 VIA** — timers, keyboard scanning, sound chip interface
- **Video ULA** — BBC's custom video logic around the 6845
- **DFS** — disk filing system for floppy images (.SSD/.DSD)

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `cpu-6502` | Shared with C64/NES | Done |
| `machine-acorn-bbc` | BBC Micro machine wiring |
| `emu198x-acorn-bbc` | GUI shell |
