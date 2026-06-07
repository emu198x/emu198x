# Acorn Atom

## Status: Boots to its prompt; keyboard types end-to-end

Acorn's £120 self-build (1980), by the team that designed the BBC Micro. Boots to
the `ACORN ATOM` banner + `>` prompt; `PRINT3` → `3`. Headless extended system.
6502 + MC6847 VDG (shared crate) + 8255 PPI.

## What works

- **Boot to prompt** (2026-06-04) with the 24K combined ROM (assembled from MAME's
  `atom` romset). `PRINT3 → 3` end-to-end.
- **VDG** — `motorola-vdg-6847` (Atom text model); PIA port A column-select, port
  B row data.

## Not implemented / accuracy gaps

- **Graphics modes 1-5** — VDG renders text only; graphics modes show solid green
  (donor stub).
- **Cassette / printer** unwired. **Snapshot** deferred. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the CPU cold-starts."** `AcornAtom::new()` never ran the 6502
  reset, so it powered on at PC=`$0000` and stuck on the uninitialised grid (same
  bug as VIC-20 + PET). Added `cpu.reset()`.
- **Note (ROM assembly):** the 24K blob was assembled from MAME `atom` —
  `abasic.ic20` low 4K → BASIC (`$C000`), high 4K → MOS (`$F000`),
  `afloat.ic21` → FP (`$D000`); `$A000` utility slot empty. Reset vector verified
  to resolve into MOS (`$FF3F`).
- **Verification target** — VDG graphics modes 1-5.

## Validated against

- MAME `atom` romset; INS8255 PPI + VDG field-sync; boot + typing verified.

## Timing & cycle-accuracy

- **Master clock & dividers** — 6502 at ~1 MHz; MC6847 VDG field-sync drives the
  display.
- **Timing model realised** — relaxed: text-mode VDG render; graphics modes 1-5
  unimplemented (solid green).
- **CPU timing** — 6502 cycle-accurate (§62).
- **Distance to full cycle-accuracy** — VDG graphics modes; beam-accurate VDG
  timing.

## Tooling & drivability

- **Script / MCP** — `--script` + `--mcp`.
- **Native window** — headless only (extended tier).
- **Disassembler** — pending the Asm198x shared 6502 disassembler.

## Peripherals & connectivity

- **Emulated now** — keyboard (8255 PPI), VDG text display.
- **Period peripherals (emulatable)** — cassette, printer, disk add-on, the Atom's
  expansion bus.
- **Internet-capable** — **Marginal**: Acorn's **Econet** was available for the
  Atom via add-on (its networking lineage predates the BBC); a real period LAN
  path, though niche.

## Crates

| Crate | Role |
|-------|------|
| `mos-6502` | CPU |
| `motorola-vdg-6847` / `intel-8255` | VDG · PPI |
| `machine-acorn-atom` / `runtime-…` / `emu198x-acorn-atom` | wiring + runner |

## ROMs

24K combined ROM at `~/.emu198x/roms/acorn-atom/atom.rom`.

## Launch

```sh
cargo run --release -p emu198x-acorn-atom -- --frames 300 --screenshot atom.png
```
