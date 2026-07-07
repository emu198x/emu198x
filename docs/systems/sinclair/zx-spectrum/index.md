# ZX Spectrum

## Status: Mainstream models complete; clones and peripherals are the remaining distance

The mainstream Sinclair/Amstrad models — **48K, 16K, +, 128K, +2, +2A, +2B, +3** —
are effectively at 100% on the hard parts (CPU, timing, video, audio) and run
games. The family also includes Russian clones (**Pentagon 128, Scorpion ZS-256**)
and US **Timex** machines (TC2048, TC2068, TS2068), which are at varying
completeness. Tape both **LOADs and SAVEs**; +3/Beta disks **read** (write is not
yet implemented). The remaining distance to a fully complete Spectrum line is
concentrated in three buckets — **disk-write + format breadth**, **the clones**,
and **the peripheral catalogue** — plus a few accuracy edges.

## What works

- **CPU** — Z80, Tom Harte 1,604,000 tests **100%**, Patrik Rak `z80test` **6/6
  with zero allowlist**, ZEXDOC/ZEXALL, FUSE 1,351/1,356 (the 5 are an accepted
  undocumented-flag allowlist — see Accuracy edges). Full undocumented behaviour
  (MEMPTR/WZ, Q flag, SCF/CCF bits 3/5).
- **Video** — ULA full display, border, **memory contention** (48K 6C001E +
  128K 7K010E phase offset, baked into a `contention_phase` field with no runtime
  conditional; the **+2A/+3 40078** has its own MREQ-only pattern in
  `amstrad-ula-40077`). Flash attribute. **Floating bus** (returns the current
  ULA fetch byte; FLOATSPY-tested). The project's *reference* timing model
  (RULES §53–56).
- **Audio** — Beeper (port `$FE` bit 4) + EAR feedback + **AY-3-8912** PSG (128K
  and later).
- **Tape** — **LOAD** (TAP, TZX with loop flattening, WAV) and **SAVE** (MIC
  capture → `.tap`, every variant in the family; see the Port `$FE` section).
- **128K paging** — port `$7FFD`: 8 RAM pages, 2 ROMs, shadow screen, paging
  lock, contended-bank detection.
- **+2A/+3 paging** — port `$1FFD`: 4-ROM selection, special all-RAM modes,
  dynamic page write-protect.
- **+3 disk (read)** — NEC µPD765A FDC (SPECIFY, SENSE DRIVE/INTERRUPT,
  RECALIBRATE, SEEK, READ DATA, READ ID) via ports `$2FFD`/`$3FFD`; real games
  load (e.g. Chase H.Q. to title). DSK/EDSK parsed (read-only).
- **Snapshots** — SNA, Z80 (v1/v2/v3, compressed + paged), RZX.
- **Input** — full keyboard matrix, Kempston joystick.
- **Capture & infra** — PNG, GIF, FFmpeg A/V, WAV; save states, rewind, turbo,
  TOML config; **CRT/LCD/raw** native filters (`emu198x-native-video`, CRT-Lottes
  shader).
- **MCP / script** — headless JSON-RPC + `--script`; rich surface (cpu/ay/memory,
  port r/w, step, run_until_pc, press_key/type_string, watch_memory/ay,
  snapshots, screenshots, `save_tape`).

## Models

| Model | ULA / core | ROMs | Paging | Storage | State |
|-------|-----------|------|--------|---------|-------|
| 16K / 48K / + | 6C001E (48K class) | 1 | none | Tape | **Complete** |
| 128K / +2 | 7K010E (128K class) | 2 | `$7FFD` | Tape | **Complete** |
| +2A / +2B / +3 | 40078 (Amstrad class) | 4 | `$7FFD`+`$1FFD` | Tape (+3: 3″ FDC) | Playable; disk read-only |
| Pentagon 128 | pentagon-ula | 2 | `$7FFD` | Tape; Beta (not wired) | Boots; no TRD load yet |
| Scorpion ZS-256 | scorpion-ula | — | `$7FFD`+`$1FFD` | Tape; Beta | **No screen output** (bug) |
| Timex TC2048 | SCLD | 1 | none | Tape | Boots; SCLD **mode 0 only** |
| Timex TC2068 / TS2068 | SCLD | 2 | bank-switch | Tape (cart) | **Does not reach menu** |

## Not implemented / accuracy gaps

### Storage (the largest bucket)
- **+3 disk WRITE** — µPD765A `WriteData` / `FormatTrack` unimplemented (read-only);
  needs the execution-phase write + an EDSK *writer* + a writable-mount/flush model.
  ST3 does not report WRPROT (bit 6).
- **Beta / TR-DOS WRITE** — WD1793 Write Sector/Track stubbed (`ST_WRITE_PROTECT`);
  near-direct port of the existing `western-digital-wd1770` write model.
- **Pentagon/Scorpion TRD LOAD** — the Beta controller exists and is wired, but no
  `MediaKind::Disk` route calls `beta.insert_disk()` and there is no `.trd`/`.scl`
  parser; most clone software is TRD, so this blocks real clone usage.
- **Formats** — **SZX** snapshot (extension allowlisted, no parser behind it),
  **CSW**/**PZX** tape, **SCL** Beta; UDI/FDI flux formats (niche).
- **128K-family tape auto-LOAD** — `autoload_basic_tape` is coupled to the 48K
  editor's K-cursor model; the 128K family boots to a menu (needs menu nav / `USR 0`).

### Clones
- **Scorpion ZS-256 — no screen output.** Boots to CPU-liveness but never paints
  `$4000–$5AFF`; three coupled memory-map bugs identified vs FUSE
  (`machine-scorpion-zs256/src/memory.rs`: `$1FFD` page-select bit, ROM-select
  logic, ROM 3 / Beta overlay).
- **Timex extended SCLD video** — modes 1–7 (hi-res 512×192, hi-colour 8×1,
  dual-screen) stored but **unrendered** (`timex-scld` renders mode 0 only); the
  Timex headline feature. TC2068/TS2068 also do not reach the boot menu; TS2068's
  in-core `frame_timing()` returns PAL despite being a 60 Hz NTSC machine.

### Peripherals (none of these are architecturally blocked — the `Peripheral`
trait + the proven ROM-paging precedent pave the road)
- **Kempston mouse**, **ZX Printer** (`$FB`), **ULAplus** (`$BF3B`/`$FF3B`
  64-colour palette), **Interface 2** (16K cart ROM + 2nd joystick), **Multiface**
  128/+3 (NMI freeze + bank-over-RAM), **Interface 1 + Microdrive** (shadow-ROM
  paging + MDR format + RS-232 + ZX Net), **Spectranet** (Ethernet — the
  cross-platform netplay target).

### Accuracy edges
- **Snow effect — not implemented** (the 128K ULA address-corruption quirk; niche).
- **ULA contention smokes are not byte-equal** to Spectron references — they pass
  at a looser self-locked-golden bar, so a subtle contention regression could slip
  through. Needs a Spectron downscale-and-compare harness.
- **+2A/+3 video/INT constants** — the *contention pattern* is +2A-specific
  (`DELAY_TABLE_PLUS2A`), but `CONFIG_PLUS2A = CONFIG_128K` aliases the video/INT
  geometry; a verification target against primary 40078 timing, likely a no-op.
- **5 FUSE block-I/O disagreements** (`INIR`/`OTIR`/`INDR`/`OTDR`, X/Y undocumented
  flag bits at the final repeat) — an accepted allowlist; real silicon itself
  varies, so effectively unclosable and zero practical impact.

## Timing & cycle-accuracy

- **Master clock** — 14 MHz master; Z80 at 3.5 MHz; the ULA ticks every
  **half-cycle** (7 MHz); `hc` is the only time counter.
- **Model** — the project's reference timing implementation (RULES §53–56): the
  master oscillator drives the loop, the ULA ticks every half-cycle, the CPU ticks
  only when the ULA allows, contention = a skipped CPU slot. Per-variant contention
  baked into `contention_phase` with no runtime conditional. **The +2A/+3 (40078)
  contention IS separately modelled** in `amstrad-ula-40077` (MREQ-only, distinct
  `DELAY_TABLE_PLUS2A`).
- **CPU** — cycle-accurate (§62): Tom Harte 100%, ZEXDOC/ZEXALL, FUSE 1,351/1,356,
  `z80test` 6/6.
- **Distance to true 100%** — the contention *logic* is settled (passes `z80test`);
  the gap is oracle strictness (byte-equal Spectron comparison, Medium) and the
  absent snow quirk (Medium, niche), not CPU or contention correctness.

## Port $FE I/O, tape SAVE/LOAD, and driving the keyboard from tests

Recurring rediscovery; documented here so it isn't re-derived each time.

**Port `$FE` bit map** (`common-sinclair-zx-spectrum/src/ula_engine.rs`):

| Bit | Write | Read |
|-----|-------|------|
| 0–2 | Border colour | keyboard half-row (active low) |
| 3 | **MIC** — tape SAVE output | — |
| 4 | **EAR / beeper** — speaker | — |
| 6 | — | **EAR input** — tape LOAD level |

MIC (bit 3) only toggles during a `SAVE`; nothing else drives it. The EAR read
(bit 6) is where tape playback feeds the loader. Both edges are captured with
T-state timing the same way the beeper level is (`sync_beeper_level`).

**Tape LOAD** = playback: a `.tap`/`.tzx` parses into a `TapeSpan` stream that
`TapePlayer` (`common-sinclair-zx-spectrum/src/tape.rs`) advances one T-state at
a time, presenting the EAR level on `$FE` bit 6.

**Tape SAVE** = capture (2026-06-08): `TapeRecorder`
(`common-sinclair-zx-spectrum/src/tape_recorder.rs`) is the mirror image — it
timestamps every MIC edge and `decode()`s the pulse train (pilot → 2 sync
pulses → two pulses per data bit, MSB first) back into standard-speed blocks.
`SpectrumRuntime::flush_tape_image()` serialises those to a reloadable `.tap`
via `format-sinclair-zx-spectrum-tap::encode_tap`; the `save_tape` MCP tool writes
it to disk. Wired across **every** variant in the family. SAVE never mutates a
mounted playback tape, so no writable-flag gating is needed (unlike disk). Repro:
`cargo test -p runtime-sinclair-zx-spectrum --test tape_save_roundtrip -- --ignored`.

**Driving the BASIC editor from a test or tool** — use a `HeadlessSession`
(`SpectrumSessionQueryProvider`) and the public `tap_key` / `tap_symbol_combo`
helpers (`runtime-sinclair-zx-spectrum`, re-exported at the crate root):

- **Cursor modes.** At the start of a line the cursor is **K** (keyword): a
  single letter key enters that key's *keyword* — `s` → `SAVE`, `j` → `LOAD`,
  `p` → `PRINT`, `e` → `REM`. After a keyword the cursor is **L** (letter), so
  letters/digits enter as typed. Type a line number with digit keys while still
  in K, then the statement keyword key. **Note:** this is the **48K** editor;
  the 128K family boots to a menu and uses a different keyword-entry model — the
  autoload/BASIC helpers are 48K-only today.
- **Symbols** needing SYMBOL SHIFT use `tap_symbol_combo` — `"` is SYMBOL SHIFT
  + `P`.
- **Compound keys** (the number-row legends) have friendly names on `press_key`:
  `Edit`, `CapsLock`, `TrueVideo`, `InvVideo`, `Up`/`Down`/`Left`/`Right`,
  `Graphics`, `Delete`, `Break`, `ExtendMode` each expand to their `CapsShift`
  chord, so `press_key("Edit")` == `press_keys ["CapsShift", "1"]` (#466).
- **`SAVE` waits.** After `SAVE "x"` + ENTER the ROM prints "Start tape, then
  press any key." and blocks until a keypress — send one before expecting signal.
- Worked example: `tests/tape_save_roundtrip.rs`.

## Peripherals & connectivity

- **Emulated now** — tape (TAP/TZX/WAV LOAD + `.tap` SAVE), +3 disk (DSK read via
  µPD765A), Beta/WD1793 (read, not yet runtime-wired for clones), Kempston
  joystick, snapshots (SNA/Z80/RZX).
- **Architecture** — a clean `Peripheral` trait
  (`common-sinclair-zx-spectrum/src/peripheral.rs`); the only recurring cost for
  new peripherals is the memory-bus ROM/RAM intercept (proven hand-wired in the
  Beta TR-DOS paging; worth generalising once 3+ ROM-paging peripherals exist).
- **Not yet** — Kempston mouse, ZX Printer, ULAplus, Interface 2, Multiface,
  Interface 1 + Microdrive, Spectranet (see the plan for sizing).
- **Internet-capable** — **Yes** (period): Interface 1 ZX Net + RS-232; modern
  emulatable kit — **Spectranet** (Ethernet), the right target for cross-platform
  netplay.

## Test coverage

Across the whole Spectrum family there are **~636 `#[test]` functions**. The
runtime crate alone has **~264** (172 integration + 92 unit), plus **8 boot
goldens** and the per-machine-crate ULA/contention/floating-bus TAP smokes
(`tape_smoke.rs`, `float_bus.rs` in the 48K/128K crates) and game goldens
(btime/ptime/halt2int/floatspy, speedlock, rainbow-islands).

Genuine coverage holes (not counting artifacts): Scorpion (liveness-only, no
golden), TC2068/TS2068 (golden locks a known-wrong stripe state, honestly), Timex
extended video modes (unimplemented → untested), Pentagon/Scorpion TRD (not
wired), +3 FDC write/format (unimplemented).

## ROMs

Place in `roms/sinclair-zx-spectrum/`:

| File | Size | Description |
|------|------|-------------|
| `48.rom` | 16KB | 48K Spectrum ROM (required for 48K) |
| `128-0.rom` / `128-1.rom` | 16KB each | 128K editor + 48K BASIC ROM |
| `plus3-0.rom` … `plus3-3.rom` | 16KB each | +3 editor / syntax / +3DOS / 48K BASIC |
