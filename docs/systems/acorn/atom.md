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
- **Keyboard SHIFT / CTRL** (2026-06-29, #372) — SHIFT and CTRL now register, on
  port B bits 7 / 6, active-low, common to every column (Atom Technical Manual
  §25.5; cross-checked against Atomulator). This makes shifted symbols typeable —
  e.g. `"` is SHIFT+2. The dedicated `*` key is still unmapped (its matrix
  position isn't in the manual or Atomulator's host-positional table).
- **Cassette LOAD — waveform path** (2026-06-29, #371) — a UEF tape decoded by
  `format-acorn-uef` plays its raw waveform onto the 8255 **PC5** cassette-data
  input (PC4 carries the free-running 2.4 kHz reference; the Atom has no serial
  receiver and no motor relay — the COS bit-bangs the level in software, 300-baud
  Kansas City). Uses the shared `common-acorn-cassette` crate's new
  `CassetteReceiver::level()` sampler rather than the byte demodulator the
  BBC/Electron use. Unit-tested (PC5 follows the tape level; `load_media` +
  reset re-mount). **OS-driven load not yet verified** — see the gap below.

## Not implemented / accuracy gaps

- **Graphics modes 1-5** — VDG renders text only; graphics modes show solid green
  (donor stub).
- **Cassette LOAD — OS-driven verification** — the PC5 waveform path is wired and
  unit-tested, but a real COS load of an Atom tape is not yet confirmed
  end-to-end. The bet (path A) is that the COS software-times the raw PC5
  waveform; an attempt to `LOAD"INSTRUCTIONS"` from the Defender tape did not yet
  land the program in RAM (the multi-key command typing was also flaky). Open:
  whether PC5 needs the raw waveform or a partially-demodulated signal — to be
  settled against the COS `CASIN` disassembly. The BBC/Electron were proven
  byte-for-byte against real ROMs; the Atom is pending that.
- **Cassette SAVE / printer** unwired. **No native window.**

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
