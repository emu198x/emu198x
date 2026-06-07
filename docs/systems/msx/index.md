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

- **Subslot expansion** — MSX1 doesn't need it; MSX2+ uses `$FFFF` writes.
  Recognised in the spec, disabled.
- **Cassette / printer** — via PPI port C, unwired. (Joystick is wired — see
  "What works".)
- **Snapshot** — deferred. **No native window.**
- **MSX2 / 2+ / TurboR** — out of scope (V9938/V9958, mapped RAM, FM-PAC).

## Known unknowns / disproven hypotheses

- **Verification targets** — VDP per-dot timing; mapper edge behaviour against
  openMSX / blueMSX.

## Validated against

- Microsoft US MSX System v1.0 + BASIC BIOS (32K, SHA-256 `3b33…d417`) → BASIC;
  `tests/bios_boot.rs`. MSX Nemesis validated clean (PSG-port-A joystick drives
  the menu).

## Timing & cycle-accuracy

- **Master clock & dividers** — 10.738635 MHz. CPU = ÷3 ≈ 3.58 MHz; VDP dot ÷2.
- **Timing model realised** — the **correct 3:2 VDP-dot phase clock** **and** the
  shared **per-dot** VDP render (each pixel drawn at its dot; `ti-tms9918::tick`).
- **CPU timing** — Z80 cycle-accurate (§62); no Z80 bus-timing oracle.
- **Distance to full cycle-accuracy** — VDP render and phase are both in place;
  remaining gaps are CPU bus-cycle timing.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp` (operational-parity rollout).
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared Z80 disassembler.

## Peripherals & connectivity

- **Emulated now** — cartridge, keyboard, joystick (PSG port A; Nemesis
  validated, `bca5802f`).
- **Period peripherals (emulatable)** — disk drives (FDC), cassette, printer,
  joysticks, mouse, the MSX-Audio / Music (FM) cartridges, RS-232 cartridges.
- **Internet-capable** — **Yes**: period RS-232 cartridges + modems; strong modern
  emulatable options — **ObsoNET** / **GR8NET** (Ethernet/WiFi cartridges with
  documented TCP stacks). One of the better-supported retro net scenes.

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
