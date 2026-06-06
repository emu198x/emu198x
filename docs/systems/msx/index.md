# MSX1

## Status: Boots to MSX BASIC, clean

Boots the Microsoft MSX BASIC prompt cleanly. Headless extended system. Z80 +
TMS9918A + AY-3-8910 (PSG) + Intel 8255 PPI — the MSX1 standard. (This folder is
the MSX family home; MSX2/2+/TurboR would be siblings.)

## What works

- **Boot to BASIC** — `MSX BASIC version 1.0 / Copyright 1983 by Microsoft /
  28815 Bytes free / Ok` on light blue. Slot 0 BIOS read, slot 3 RAM, PPI port A
  slot select, TMS9918A text render all exercised by real BIOS code. Smoke
  `tests/bios_boot.rs` (1/1).
- **Memory-slot system** — PPI port A → primary slot per 16K page.
- **MegaROM mappers** — Plain / Konami / Konami SCC / ASCII 8 / ASCII 16.
- **Correct 3:2 VDP-dot phase clock.**

## Not implemented / accuracy gaps

- **TMS9918A scanline-batched render** (shared with Coleco/SG-1000).
- **Subslot expansion** — MSX1 doesn't need it; MSX2+ uses `$FFFF` writes.
  Recognised in the spec, disabled.
- **Joystick / cassette / printer** — PSG R14/R15 joystick hookup exists chip-side
  but no machine input surface; cassette/printer via PPI port C unwired.
- **Snapshot** — deferred. **No native window.**
- **MSX2 / 2+ / TurboR** — out of scope (V9938/V9958, mapped RAM, FM-PAC).

## Known unknowns / disproven hypotheses

- **Open: joystick surface** — chip side ready, machine side not wired.
- **Verification targets** — VDP per-dot timing; mapper edge behaviour against
  openMSX / blueMSX.

## Validated against

- Microsoft US MSX System v1.0 + BASIC BIOS (32K, SHA-256 `3b33…d417`) → BASIC;
  `tests/bios_boot.rs`. MSX Nemesis validated clean (PSG-port-A joystick drives
  the menu).

## Crates

| Crate | Role |
|-------|------|
| `zilog-z80` | CPU |
| `ti-tms9918` / `gi-ay-3-8912` / `intel-8255` | VDP · PSG · PPI |
| `machine-msx` / `runtime-msx` / `emu198x-msx` | wiring + runner |

## ROMs

32K BIOS at `~/.emu198x/roms/microsoft-msx/msx.rom` (real BIOS from TOSEC) or
`cbios_main_msx1.rom` (free C-BIOS).

## Launch

```sh
cargo run --release -p emu198x-msx -- --bios msx.rom --frames 200 --screenshot msx.png
```
