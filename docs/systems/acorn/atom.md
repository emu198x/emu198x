# Acorn Atom

## Status: Boots, types, loads software, plays sound, and renders graphics

Acorn's £120 self-build (1980), by the team that designed the BBC Micro. Boots to
the `ACORN ATOM` banner + `>` prompt; `PRINT3` → `3`. Headless extended system.
6502 + MC6847 VDG (shared crate) + 8255 PPI.

## What works

- **Boot to prompt** (2026-06-04) with the 24K combined ROM (assembled from MAME's
  `atom` romset). `PRINT3 → 3` end-to-end.
- **VDG** — `motorola-vdg-6847`; PIA port A column-select, port B row data.
- **Graphics modes 1-5** (2026-06-29, #367) — all eight MC6847 modes render, not
  just text. Port A **PA4 = A/G** and **PA5-7 = GM0-2** (MAME `atom.cpp`) now decode
  into the shared crate's `VdgControl` (the old code read bit 7 for A/G, so every
  graphics mode fell through to a flat field). Video RAM expanded 1 KB → **6 KB**
  (`$8000-$97FF`) for the 256×192 modes; `$9800-$9FFF` is open bus. INT/EXT is tied
  low (internal font). Verified by unit tests (A/G selection, GM bits, 6 KB read,
  spatial pattern) and a headless screenshot — a graphics program drawing `$AA`
  renders crisp vertical stripes, not solid green.
- **CSS sourced from PC3** (2026-06-29, #369) — the MC6847 colour-set select now
  reads 8255 **port C bit 3**, not port-A bit 3 (PA3), which is a keyboard-scan
  line (Atom Technical Manual §25.5; Atomulator). Handles both the direct `$B002`
  write and the BSR control-word path (`$B003`). Latent until graphics modes are
  wired (#367), but the colour-set is now correct rather than reading the keyboard.
- **Keyboard — all 62 keys** (2026-06-29, #372) — SHIFT and CTRL register on port
  B bits 7 / 6 (active-low, common to every column; Atom Technical Manual §25.5),
  and every key was mapped against the real MOS. The Atom puts its symbols on
  shifted keys like a typewriter, so `*` (the COS command prefix) is **SHIFT+`:`**,
  `"` is SHIFT+2, `+` is SHIFT+`;`, etc. — the runtime input maps each symbol to
  SHIFT+base. Also wired: the base keys `- [ ] \ ↑` (the Atom draws ASCII 0x5E as
  an up-arrow, modern `^`), DELETE, ESC, LOCK (shift-lock), REPT (auto-repeat, on
  port C bit 6 — not the scanned matrix), the two bidirectional cursor keys (↑/↓
  and →/←, SHIFT reversing the direction — the Atom has no arrow-key cluster), and
  **COPY** — the Acorn screen editor's key that reads the character under the copy
  cursor into the input. COPY was found by tracing the code the MOS emits at
  OSWRCH (it copies the character under the cursor rather than a fixed code, which
  is why behavioural probing couldn't see it). All verified by `#[ignore]` MOS
  tests.
- **`.atm` program loading** (2026-06-29, #366) — the `format-acorn-atom-atm`
  crate parses the Wouter Ras `.atm` header (16-byte name + LE load/exec/length)
  and the runtime's `program-1` slot (`MediaKind::Program`) injects the body into
  RAM at the load address, auto-running programs (exec in low RAM) and load-only
  for screen images (exec in video RAM). `AtomFull` RAM bumped to a fully-expanded
  32 KB so programs at `$2800+` fit. Verified in CI with a synthetic `.atm` that
  loads + auto-runs, and against real archive files (`EMU198X_ATOM_ATM`). The
  archive's binaries are `.atm`-format even without the extension (e.g. MENU loads
  at `$2800`).
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
- **1-bit speaker audio** (2026-06-29, #368) — the loudspeaker on 8255 **PC2**
  (programs toggle it with `?#B002 EOR 4`; *Atomic Theory and Practice* §19) is
  sampled each master tick into an `f32` waveform, downsampled 1 MHz → 48 kHz with
  a fractional accumulator and drained into the runtime's `AudioPacket` (which had
  been pushing an empty buffer). High PC2 = `+0.5`, low = `-0.5`. Verified in CI: a
  toggler loaded as a `.atm` yields a non-empty waveform end-to-end (machine +
  runtime tests).

## Not implemented / accuracy gaps

- **Beam-accurate VDG** — graphics render per-frame from a video-RAM snapshot, so
  mid-frame mode/palette changes (split-screen effects) aren't yet honoured.
- **Cassette SAVE / printer** unwired. **No native window.**

## Known unknowns / disproven hypotheses

- **DISPROVEN: "the CPU cold-starts."** `AcornAtom::new()` never ran the 6502
  reset, so it powered on at PC=`$0000` and stuck on the uninitialised grid (same
  bug as VIC-20 + PET). Added `cpu.reset()`.
- **Note (ROM assembly):** the 24K blob was assembled from MAME `atom` —
  `abasic.ic20` low 4K → BASIC (`$C000`), high 4K → MOS (`$F000`),
  `afloat.ic21` → FP (`$D000`); `$A000` utility slot empty. Reset vector verified
  to resolve into MOS (`$FF3F`).
- **Verification target** — beam-accurate VDG (mid-frame mode changes).

## Validated against

- MAME `atom` romset; INS8255 PPI + VDG field-sync; boot + typing verified.
- **CI** (#370): `keyboard_scan.rs` boots a hand-assembled synthetic ROM and has
  the 6502 scan the 8255 keyboard — covering the boot+keyboard wiring without the
  copyrighted MOS (which is never bundled; Tier 3 of `test-rom-policy`). The
  real-MOS boot/keyboard/cassette/COPY tests stay `--ignored`, run locally with
  `EMU198X_ATOM_ROM`.

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
- **Timing model realised** — relaxed: per-frame VDG render (text + all graphics
  modes) from a video-RAM snapshot; not yet beam-accurate.
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
