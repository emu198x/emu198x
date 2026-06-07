# Acorn Electron

## Status: Boots to BASIC `>`; keyboard types

Boots to `Acorn Electron` / `BASIC` / `>` and types (`PRINT 123` executes).
Headless extended system. 6502 + the Electron's custom ULA (inline) — no new chip
crate.

## What works

- **Boot to BASIC** (2026-06-04) with the OS ROM (16K, SHA-256 `b63f…cda4`, TOSEC)
  + Acorn BASIC II (byte-identical to BBC Model B BASIC II, md5 `2cc6…8e3d`).
- **ULA** — BBC-compatible 8-colour palette, 8 display modes (0-6; no MODE 7
  teletext), `$FE00` interrupt control, VBlank + RTC IRQ sources, ULA tone sound.
- **Keyboard** — types into BASIC, read the accurate way: through the paged
  region (`$8000-$BFFF`, ROM slot 8/9), not `$FE00` (fix `5d4b1d87`; test
  `keyboard_reads_active_high_through_paged_rom`).

## Not implemented / accuracy gaps

- **ULA bus contention** — CPU halves to 1 MHz during ULA RAM-fetch windows; this
  port runs a flat 2 MHz. A significant gap on contention-sensitive software
  (Elite, scrollers).
- **Sideways ROM paging (`$FE05`)** — register stored but doesn't swap a paged-ROM
  array in; only BASIC visible.
- **Cassette (`$FE04`)** write-stub. **Snapshot** deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the interrupt model is fine."** `$FE00` is the IRQ *enable*, not a
  clear — the frozen-interrupt model was wrong (fixed 2026-06-05).
- **DISPROVEN (donor ULA), three fixes vs MAME `electron_ula`:** (1) palette
  register format is scrambled + inverted (`written ^ 0xFF`), not a nibble — the
  stub painted the screen red; (2) `$FE02/$FE03` pack address bits A14-A6, not raw
  high/low — naive decode scanned RAM garbage; (3) each 8×8 cell is 8 consecutive
  bytes, text rows 10 lines apart (250 displayed) — the old raster stride was
  wrong.
- **Verification target** — ULA contention timing (the big accuracy gap).

## Validated against

- MAME `electron_ula` — palette, screen-start, display layout.
- OS ROM (TOSEC) + BASIC II → `>`; `PRINT 123` executes.

## Timing & cycle-accuracy

- **Master clock & dividers** — 16 MHz master; 6502 nominally 2 MHz, but real
  hardware **halves to 1 MHz during ULA RAM-fetch windows**.
- **Timing model realised** — relaxed: runs a **flat 2 MHz** with no ULA
  contention — a significant gap on a machine whose software (Elite, scrollers)
  is heavily timing-sensitive. Display layout/screen-start now MAME-accurate.
- **CPU timing** — 6502 cycle-accurate (§62) at the instruction level; the bus
  contention slowdown is the missing piece.
- **Distance to full cycle-accuracy** — ULA 1 MHz/2 MHz contention.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`.
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared 6502 disassembler.

## Peripherals & connectivity

- **Emulated now** — keyboard, ULA display + sound.
- **Period peripherals (emulatable)** — the **Plus 1** (printer + serial + cart
  slots), Plus 3 disk, cassette, joysticks.
- **Internet-capable** — **Marginal**: no native Econet (unlike the BBC); Plus 1
  serial + third-party comms add-ons existed. A modern serial bridge is possible.

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU |
| `machine-acorn-electron` (ULA inline) / `runtime-…` / `emu198x-acorn-electron` | wiring + runner |

## ROMs

OS + BASIC II at `~/.emu198x/roms/acorn-electron/` (`basic.rom`).

## Launch

```sh
cargo run --release -p emu198x-acorn-electron -- --frames 300 --screenshot elk.png
```
