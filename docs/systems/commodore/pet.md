# Commodore PET

## Status: Boots to BASIC `READY.`; keyboard wired

The 1977 business machine (one of the "1977 trinity"). Boots to `### COMMODORE
BASIC ###` / `31743 BYTES FREE` / `READY.`. Headless extended system. 6502 +
PIA-6520 + VIA-6522 + 6845 CRTC — all reused, no new chip crate.

## What works

- **Boot to BASIC** (2026-06-04) with the VICE 901465-* ROM set (BASIC 2 + Kernal
  2 + Editor 2N) + 4K character ROM. Smoke asserts the banner + free-bytes.
- **Keyboard** — PIA port A column-select, port B row read (10×8 matrix); the
  editor's vertical-retrace spin-wait is satisfied via VIA PB5 ← CRTC retrace
  (2026-06-05).
- **CRTC** — 40/80-column geometry, latched `ma_output` so the displayed char
  isn't dropped.

## Not implemented / accuracy gaps

- **Cassette / IEEE-488** unwired (VIA exists, external lines not connected).
- **Speaker** (VIA CB2 piezo) unwired.
- **`.prg` / `.tap` load** not implemented. **Snapshot** deferred. **No native
  window.**
- **80-column CRTC clock** — runs at 1× CPU; real 80-column hardware clocks the
  CRTC at 2 MHz (donor-v1 simplification).

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the CPU cold-starts."** `Pet::new()` left the 6502 at PC=`$0000`,
  running the BRK there instead of the `$FFFC` reset vector. Added `cpu.reset()`
  (the C64/5200 do the same).
- **DISPROVEN (donor): "glyph stride is 16 bytes."** The PET glyph ROM is 8
  bytes/char; `code*16` made every glyph read its neighbour and "spaces" fetch a
  non-blank glyph (horizontal-line noise). Now `code*8 + scanline`.
- **DISPROVEN (`motorola-6845`): "sample `ma` after advancing."** The CRTC
  pre-incremented, dropping the first cell of every row (banner lost its leading
  `*`). Now latches `ma_output` and advances behind it.
- **Verification target** — 80-column CRTC timing.

## Validated against

- VICE 901465-* ROM set → `READY.`; keyboard matrix ground-truthed (CB1 retrace
  IRQ + scan, 2026-06-05). Reference: VICE.

## Timing & cycle-accuracy

- **Master clock & dividers** — 6502 at ~1 MHz (8 MHz crystal ÷ 8); CRTC ticks at
  the same 1× rate (donor-v1 simplification — real 80-column hardware clocks the
  CRTC at 2 MHz).
- **Timing model realised** — relaxed: one master tick = one 6502 cycle = one CRTC
  tick; vertical-retrace is reflected via VIA PB5 so the editor's spin-wait
  releases, but the 80-column 2 MHz CRTC clock isn't modelled.
- **CPU timing** — 6502 cycle-accurate (§62).
- **Distance to full cycle-accuracy** — 80-column CRTC at 2 MHz; exact CRTC
  character timing.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`.
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared 6502 disassembler.

## Peripherals & connectivity

- **Emulated now** — keyboard; CRTC display.
- **Period peripherals (emulatable)** — IEEE-488 drives (2040/4040/8050), datasette,
  printers, the PET's expansion/userport.
- **Internet-capable** — **Marginal-to-Yes**: RS-232 + IEEE-488-attached modems
  gave period BBS access; no prioritised modern device, but a serial-modem bridge
  is realistic.

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU |
| `mos-pia-6520` / `mos-via-6522` / `motorola-6845` | PIA · VIA · CRTC |
| `machine-commodore-pet` / `runtime-…` / `emu198x-commodore-pet` | wiring + runner |

## ROMs

BASIC2 + Kernal2 + Editor2N + character ROM at `~/.emu198x/roms/commodore-pet/`.

## Launch

```sh
cargo run --release -p emu198x-commodore-pet -- --frames 200 --screenshot pet.png
```
