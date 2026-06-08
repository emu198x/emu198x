# ZX Spectrum

## Status: Playable (48K, 128K, +2A, +3)

The Spectrum boots, loads tapes, plays games, and has full audio. The 128K model has bank switching, AY sound, and shadow screen. The +2A/+3 models add extended paging (port $1FFD), 4 ROM banks, and the +3 includes an FDC with DSK disk image support.

## What works

- **CPU:** Z80 — 1,604,000 Tom Harte tests passing. Full undocumented behaviour (MEMPTR, Q flag, SCF/CCF bits 3/5).
- **Video:** ULA — full display with border, memory contention (48K 6C001E pattern + 128K 7K010E phase offset, baked into contention_phase field with no runtime conditional). Flash attribute. Floating bus (returns current ULA fetch byte). Snow effect (CPU reads from display RAM corrupt ULA address).
- **Audio:** Beeper (port $FE bit 4) + MIC (bit 3) + EAR feedback + AY-3-8912 PSG (128K).
- **128K paging:** Port $7FFD — 8 RAM pages, 2 ROMs, shadow screen, paging lock. Contended memory detection for odd-numbered banks at $C000.
- **+2A/+3 paging:** Port $1FFD — normal mode with 4-ROM selection ($1FFD bit 2 + $7FFD bit 4), special mode with 4 all-RAM configurations. Page table write protection toggled dynamically.
- **+3 FDC:** NEC uPD765A — SPECIFY, SENSE DRIVE STATUS, RECALIBRATE, SENSE INTERRUPT STATUS, SEEK, READ DATA commands. Accessed via ports $2FFD (status) and $3FFD (data). Motor control via $1FFD bit 3.
- **Disk images:** DSK format parser (standard + extended) with track/sector addressing and variable sector sizes.
- **Tape loading:** TAP, TZX (with loop flattening), WAV import via emu-tape.
- **Snapshots:** SNA and Z80 (v1/v2/v3, compressed and paged).
- **Input:** Full keyboard matrix, Kempston joystick.
- **Capture:** PNG screenshots, GIF recording, FFmpeg video+audio, WAV audio recording.
- **Infrastructure:** Save states, rewind (Tab), turbo mode (F1), TOML config persistence.
- **MCP server:** Headless JSON-RPC 2.0 (cpu_state, memory_peek, step, screenshot, etc.).
- **Shell:** Auto-detects +3 ROMs (4 files) > 128K ROMs (2 files) > 48K ROM. Auto-LOAD for tapes, .dsk file support, ZIP extraction.

## Models

| Model | ULA | ROMs | Paging | Storage |
|-------|-----|------|--------|---------|
| 48K | 6C001E | 1 | None | Tape |
| 128K | 7K010E | 2 | $7FFD | Tape |
| +2 | 7K010E | 2 | $7FFD | Tape (built-in) |
| +2A | 40078 | 4 | $7FFD + $1FFD | Tape |
| +3 | 40078 | 4 | $7FFD + $1FFD | Tape + 3" FDC |

## Not implemented / accuracy gaps

### Important
- **128K tape loading** — auto-LOAD needs the `USR 0` sequence for 128K mode entry
- **SZX snapshot format** — third-party snapshots often use this
- **CRT shader** — WGSL fragment shader implementing `CrtParameters`

### Nice to have
- **PZX/CSW tape formats** — niche but some titles only available in these
- **Multiface/Interface 1** — peripheral emulation
- **True +3 disk operations** — WRITE DATA, FORMAT TRACK commands for FDC
- **+3-specific contention** — the 40078 gate array has slightly different timing
  from the 7K010E; not separately modelled.
- **Scorpion ZS-256** — reaches CPU-liveness but not screen output (research
  recorded, fix scoped).

## Known unknowns / disproven hypotheses

- **Open: ULA smoke strictness.** The 5 ULA/contention TAP smokes aren't yet
  tightened to strict Spectron PNG comparison — they pass at a looser bar, so a
  subtle contention regression could slip through. (Noted in
  `docs/status/current-system-usability.md` as residual debt, not a launch gate.)
- **Open: +3 contention model.** Assumed close to the 7K010E; the 40078's actual
  timing is a verification target against primary ULA timing docs.
- **Verification targets** — the contention patterns (6C001E / 7K010E phase
  offset) are validated by TAP smokes and CPU oracles; the underlying timing
  numbers should be confirmed against the primary ULA references in
  `../../reference/` rather than treated as settled.

## Validated against

- CPU: Tom Harte 100%, ZEXDOC/ZEXALL pass, FUSE 1,351/1,356, Patrik Rak
  `z80test` 6/6 (zero allowlist).
- 262/262 runtime tests; 8/8 boot goldens; 6 ULA/contention TAP smokes.
- Reference: fuse, zesarux, SpecIde, Spectrum MiSTer core (`emulators/zx-spectrum/`).

## Timing & cycle-accuracy

- **Master clock & dividers** — 14 MHz master; Z80 at 3.5 MHz; the ULA ticks every
  **half-cycle** (7 MHz). `hc` is the only time counter.
- **Timing model realised** — the **reference implementation of the project's
  model** (RULES §53-56 is written around it): the master oscillator drives the
  loop, the ULA ticks every half-cycle, the CPU ticks only when the ULA allows,
  contention = a skipped CPU slot. Per-variant contention (48K 6C001E / 128K
  7K010E phase offset) is baked into a `contention_phase` field with no runtime
  conditional. Floating bus + snow effect modelled.
- **CPU timing** — Z80 cycle-accurate (§62): Tom Harte 100%, ZEXDOC/ZEXALL, FUSE
  1,351/1,356, Patrik Rak `z80test` 6/6.
- **Distance to full cycle-accuracy** — the +3 (40078 gate array) has slightly
  different contention timing, not separately modelled; the 5 ULA smokes aren't
  yet byte-equal against Spectron references.

## Tooling & drivability

- **Script / MCP** — strong: `--script` + a full JSON-RPC `--mcp` (cpu_state,
  memory_peek/poke, port r/w, step, run_until_pc, press_key/type_string,
  query_ay, watch_memory/watch_ay, snapshots, screenshots).
- **Native window** — yes (primary tier): `wgpu` `raw`/`lcd`/`crt`, keyboard,
  audio, tape autoload.
- **Disassembler** — `disasm` present; converges on the Asm198x shared Z80
  disassembler.

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
via `format-sinclair-zx-spectrum-tap::encode_tap`. Wired across all three core
classes, so every mainstream model captures: **48K / 16K / +** (48K class),
**128K / +2** (128K class), and **+2A / +2B / +3** (Amstrad class, one generic
impl). The bespoke clone cores — Pentagon, Scorpion, Timex TC2048/TS2068 — have
their own structs and inherit the no-op default (not yet wired). Standard pulse
constants live in
`tape.rs`; SAVE never mutates a mounted playback tape, so no writable-flag
gating is needed (unlike disk). Repro: `cargo test -p runtime-sinclair-zx-spectrum
--test tape_save_roundtrip -- --ignored`.

**Driving the 48K BASIC editor from a test or tool** — use a `HeadlessSession`
(`SpectrumSessionQueryProvider`) and the public `tap_key` / `tap_symbol_combo`
helpers (`runtime-sinclair-zx-spectrum`, re-exported at the crate root). Key
points:

- **Cursor modes.** At the start of a line the cursor is **K** (keyword): a
  single letter key enters that key's *keyword* — `s` → `SAVE`, `j` → `LOAD`,
  `p` → `PRINT`, `e` → `REM`. After a keyword the cursor is **L** (letter), so
  letters/digits enter literally. Type a line number with digit keys while still
  in K, then the statement keyword key.
- **Symbols** needing SYMBOL SHIFT use `tap_symbol_combo` — e.g. `"` is SYMBOL
  SHIFT + `P` (`tap_symbol_combo(s, "p")`).
- **`SAVE` waits.** After `SAVE "x"` + ENTER the ROM prints "Start tape, then
  press any key." and blocks until a keypress — send one before expecting the
  signal.
- Worked example: `tests/tape_save_roundtrip.rs` (enter `10 REM`, `SAVE "A"`,
  flush, assert the `.tap` header + data block).

## Peripherals & connectivity

- **Emulated now** — tape (TAP/TZX/WAV), +3 disk (DSK via uPD765A FDC), Kempston
  joystick, snapshots (SNA/Z80).
- **Period peripherals (emulatable)** — Interface 1 (microdrive + RS-232 + ZX Net),
  Interface 2 (carts + joystick), ZX Printer, Multiface, Beta Disk.
- **Internet-capable** — **Yes**: Interface 1 carried **ZX Net** (a period local
  network) and RS-232; modern emulatable kit — **Spectranet** (Ethernet, fully
  documented) and ESP-based WiFi modems. A strong, active net scene.

## Test coverage

| Component | Tests |
|-----------|-------|
| ULA | 25 (frame timing, contention patterns, 128K banks, phase offset, floating bus, snow effect, display rendering) |
| AY-3-8912 | 5 (tone period, register mapping, audio output) |
| DSK | 5 (standard format parsing, sector read, error handling) |
| FDC | 5 (status, specify, recalibrate, seek, read sector) |
| Machine | 10 (ROM, paging, keyboard, save state) |
| Snapshots | 10 (SNA, Z80 v1/v2/v3) |
| Variants | 6 (48K, 128K, +2, +2A, +3 registration) |
| **Total** | **105** |

## ROMs

Place in `roms/sinclair-zx-spectrum/`:

| File | Size | Description |
|------|------|-------------|
| `48.rom` | 16KB | 48K Spectrum ROM (required for 48K) |
| `128-0.rom` | 16KB | 128K editor ROM (enables 128K mode) |
| `128-1.rom` | 16KB | 48K BASIC ROM for 128K |
| `plus3-0.rom` | 16KB | +3 editor ROM (enables +3 mode) |
| `plus3-1.rom` | 16KB | +3 syntax checker ROM |
| `plus3-2.rom` | 16KB | +3DOS ROM |
| `plus3-3.rom` | 16KB | 48K BASIC ROM for +3 |
