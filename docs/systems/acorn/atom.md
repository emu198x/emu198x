# Acorn Atom

## Status: Boots, types, and loads software from cassette

Acorn's £120 self-build (1980), by the team that designed the BBC Micro. Boots to
the `ACORN ATOM` banner + `>` prompt; `PRINT3` → `3`. Headless extended system.
6502 + MC6847 VDG (shared crate) + 8255 PPI.

## What works

- **Boot to prompt** (2026-06-04) with the 24K combined ROM (assembled from MAME's
  `atom` romset). `PRINT3 → 3` end-to-end.
- **VDG** — `motorola-vdg-6847` (Atom text model); PIA port A column-select, port
  B row data.
- **CSS sourced from PC3** (2026-06-29, #369) — the MC6847 colour-set select now
  reads 8255 **port C bit 3**, not port-A bit 3 (PA3), which is a keyboard-scan
  line (Atom Technical Manual §25.5; Atomulator). Handles both the direct `$B002`
  write and the BSR control-word path (`$B003`). Latent until graphics modes are
  wired (#367), but the colour-set is now correct rather than reading the keyboard.
- **Keyboard SHIFT / CTRL** (2026-06-29, #372) — SHIFT and CTRL now register, on
  port B bits 7 / 6, active-low, common to every column (Atom Technical Manual
  §25.5; cross-checked against Atomulator). This makes shifted symbols typeable —
  e.g. `"` is SHIFT+2. The dedicated `*` key is still unmapped (its matrix
  position isn't in the manual or Atomulator's host-positional table).
- **Cassette LOAD — verified end-to-end** (2026-06-29, #371) — a UEF tape decoded
  by `format-acorn-uef` plays its raw waveform onto the 8255 **PC5** cassette-data
  input (PC4 carries the free-running 2.4 kHz reference; the Atom has no serial
  receiver and no motor relay — the COS bit-bangs the level in software, 300-baud
  Kansas City). Uses the shared `common-acorn-cassette` crate's new
  `CassetteReceiver::level()` sampler rather than the byte demodulator the
  BBC/Electron use. **Proven OS-driven** (`tape_load.rs`, `#[ignore]`): booting the
  real ROM, typing `LOAD"INSTRUCTIONS"` (the `"` via SHIFT+2), and playing the
  Defender tape, the COS software-decodes PC5 and the program lands in RAM. This
  confirmed path A — the COS times the raw waveform; no hardware demodulation
  needed. `load_media` + reset re-mount also covered.

## Not implemented / accuracy gaps

- **Graphics modes 1-5** — VDG renders text only; graphics modes show solid green
  (donor stub).
- **Cassette SAVE / printer** unwired. **No native window.**
- **The `*` key** is unmapped (its matrix position isn't in the manual or
  Atomulator), so COS `*`-commands can't be typed yet; BASIC `LOAD"…"` works.

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
- **Field-sync & cassette tone provenanced** (2026-06-29, #373) — PC7 field-sync
  is a 50 Hz PAL field (192 active lines of 312), **high during active video and
  low during flyback** per the Atom Technical Manual / *Atomic Theory and
  Practice* §25.5 (the earlier approximation had the polarity inverted). PC4's
  2.4 kHz reference is the 4 MHz crystal ÷1667, 50% duty. All three are now named
  constants with citations; keyboard typing + cassette load are unaffected. Tying
  the field to the VDG's own frame counter waits on the graphics-mode work (#367).
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
