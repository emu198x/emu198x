# Jupiter Ace

## Status: Boots to Forth; fully interactive (types and executes)

The Forth-instead-of-BASIC machine from the ZX Spectrum ROM team (Vickers +
Altwasser). Boots to its cursor, types on the bottom Forth input line, and
executes (ENTER interprets the line). Headless extended system. Z80A + 8K ROM +
character display — no new chip crate (Spectrum-style keyboard).

## What works

- **Boot + full interactivity** (2026-06-05) — types (chars echo on the bottom
  input line) and **executes** (`hello` → `?hello` undefined-word error).
- **Memory map** (per MAME `cantab/jupace.cpp`) — ROM `$0000-$1FFF`, video RAM
  `$2000-$23FF`, character RAM `$2800-$2BFF` (128 user-redefinable glyphs), 1K
  user RAM mirrored `$3000-$3FFF`; A10-ignored mirrors at `$2400`/`$2C00`.
- **Keyboard + beeper** — port `$FE` bit 0 read (row in high addr byte), beeper on
  bit 4 write.
- **The steady inverse-block cursor is correct** — a mode indicator (ZX81-family),
  not a flashing cursor.

## Not implemented / accuracy gaps

- **Audio** — mono beeper buffer taken via `take_audio_buffer()` but no WAV.
- **`.ace` snapshot load** — donor handled it; not yet ported (RAM dump at `$2000`).
- **Snapshot** — deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the keyboard register read is buggy."** The long key-reg dive was
  a measurement error — the Ace's Forth input line is at the *bottom* of the
  screen (row 23), not the top.
- **DISPROVEN (crate): video/char RAM placement.** The crate had video and char
  RAM swapped — the `$2400` video mirror treated as char RAM, real char RAM
  (`$2800`) routed to general RAM — so the screen-clear landed in the font
  (every cell glyph 0 = a vertical line) and the font copy went to dead RAM.
  Fixed to match MAME's map.
- **Root cause of non-interactivity (resolved):** the 50 Hz interrupt servicing —
  the half-cycle `zilog-z80` is now driven 2×/T-state with a held
  (acknowledge-cleared) INT instead of a fixed window (commit `26b957d5`).

## Validated against

- MAME `cantab/jupace.cpp` — memory map + mirrors.
- Standard 8K ROM (md5 `db6e…fc3c`, from `emulators/zx-spectrum/.../jupiter.rom`);
  types + executes verified.

## Timing & cycle-accuracy

- **Master clock & dividers** — Z80A at 3.25 MHz. PAL: 312 lines × 207
  T-states/line = 64,584 T-states/frame ≈ 50.3 Hz; INT pulsed for the first 32
  T-states of each frame.
- **Timing model realised** — closer to `hc`-driven than most donors: the
  half-cycle `zilog-z80` is driven **2× per T-state** with a held
  (acknowledge-cleared) INT — the fix that made it interactive. The character
  display still renders **end-of-frame** (mid-frame char/charset writes not shown
  until the next frame).
- **CPU timing** — Z80 cycle-accurate (§62).
- **Distance to full cycle-accuracy** — beam-accurate display (the ULA-equivalent
  bus-stealing during refresh), mid-frame character changes.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — ROM, keyboard, character display (user-redefinable glyphs).
- **Period peripherals (emulatable)** — cassette, RAM packs, the Ace's minimal
  expansion connector.
- **Internet-capable** — **No**: a minimal 1982 Forth machine with no period or
  practical modern net path.

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `machine-jupiter-ace` / `runtime-…` / `emu198x-jupiter-ace` | wiring (inline display/keyboard) + runner |

## ROMs

8K ROM at `~/.emu198x/roms/jupiter-ace/ace.rom`.

## Launch

```sh
cargo run --release -p emu198x-jupiter-ace -- --frames 200 --screenshot ace.png
```
