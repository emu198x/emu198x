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
- **ULA bus contention** (2026-06-08) — the CPU drops to **1 MHz on every RAM
  (`$0000-$7FFF`) and keyboard-paged-ROM access**, 2 MHz for ROM/OS/I/O. The
  frame is a fixed 312 × 128 master ticks at 2 MHz; the CPU fits a variable
  number of 6502 cycles into it by its RAM/ROM access mix — so RAM-bound code
  runs at half the speed of ROM code, the Electron's defining trait. Matches
  MAME `electron_ula::set_cpu_clock`; tests `ram_access_costs_two_master_ticks…`,
  `ram_bound_code_fits_fewer_cpu_cycles…`.
- **Modes-0-3 display-fetch halt** (2026-06-08) — on top of the 1 MHz drop, a RAM
  access in a graphics mode 0-3 while the video circuits are fetching (the first
  40 µs / 80 master ticks of a displayed scanline) *halts* the CPU's clock until
  the fetch window ends — the Electron's harshest contention, why mode-0 code
  crawls. Faithful to MAME `electron_ula::waitforramsync` (stall
  `(640 − hpos)/16` rounded up to even for the ½ MHz resync), in
  `display_halt_ticks`; tests `display_halt_only_bites_ram_in_modes_0_3_fetch_window`,
  `ram_resident_loop_crawls_in_mode0_versus_mode4`.

## Not implemented / accuracy gaps

- **ULA contention — half-cycle sync penalty.** Both the 1 MHz RAM/keyboard-ROM
  drop and the modes-0-3 display-fetch *halt* are now modelled (see "What works").
  Still missing: the half-cycle penalty when the 2 MHz→1 MHz clocks resynchronise
  on *every* clock change (the fetch-halt path already carries its own ½ MHz
  even-rounding); and excluding mode 3's spaced blank lines from the halt window
  (a documented MAME limitation too — `waitforramsync` over-halts those lines).
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
- **Verification target** — the modes-0-3 display-fetch halt and the base 1 MHz
  RAM contention are both in (per MAME `electron_ula`); a real mode-0 timing demo
  against MAME / Elkulator captures would confirm the exact stall edges.

## Validated against

- MAME `electron_ula` — palette, screen-start, display layout.
- OS ROM (TOSEC) + BASIC II → `>`; `PRINT 123` executes.

## Timing & cycle-accuracy

- **Master clock & dividers** — 16 MHz master; 6502 nominally 2 MHz, but real
  hardware **halves to 1 MHz during ULA RAM-fetch windows**.
- **Timing model realised** — master-clock-driven: a fixed 312 × 128 master
  ticks/frame at 2 MHz, the CPU spending one tick per ROM/OS/I/O cycle and two
  per RAM / keyboard-ROM cycle — the **1 MHz RAM contention** that gives the
  Electron its character — *plus* the **modes-0-3 display-fetch halt**, which
  stops the CPU's clock to the end of the fetch window on a contended RAM access
  during the active display (`display_halt_ticks`, per MAME `waitforramsync`).
  Display layout/screen-start MAME-accurate.
- **CPU timing** — 6502 cycle-accurate (§62) at the instruction level; bus
  contention modelled at access-class (RAM/ROM) granularity plus the per-scanline
  display-fetch halt for modes 0-3.
- **Distance to full cycle-accuracy** — the half-cycle 2→1 MHz clock-resync
  penalty on clock changes outside the fetch halt; excluding mode 3's spaced
  blank lines from the halt window.

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
