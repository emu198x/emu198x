# Dragon Accuracy Audit

Last updated: 2026-05-04.

This audit is about our own Dragon 32/64 emulation accuracy, not matching another
emulator. Other emulators are still useful as smoke references, but hardware and
Motorola/Dragon source material are the authority when behavior differs.

## Primary Sources

- `docs/source-extracts/dragon-primary/mc6809-mc6809e-programming-manual-1981.txt`
- `docs/source-extracts/dragon-primary/mc6809e-hmos-microprocessor-1984.txt`
- `docs/source-extracts/dragon-primary/mc6821-pia-1985.txt`
- `docs/source-extracts/dragon-primary/mc6847-video-display-generator-1984.txt`
- `docs/source-extracts/dragon-primary/mc6883-sam-advance-sheet.txt`
- `docs/source-extracts/dragon-primary/sam-programming-guide.txt`
- Dragon Archive memory map:
  <https://worldofdragon.org/index.php?title=Memory_Map>
- On A Stick Dragon memory map:
  <https://www.onasticksoftware.co.uk/dragon/sys4.htm>

## Source-Aligned Behavior

- The machine master clock is source-aligned at 14.31818 MHz. The SAM advance
  sheet describes the typical crystal at this rate and derives slow CPU cycles
  from crystal/16. Our `DRAGON_MASTER_HZ`, `SLOW_CPU_MASTER_TICKS`, and
  `DRAGON_CPU_HZ` constants in `crates/machine-dragon-32/src/lib.rs` follow
  that model.
- The address-dependent SAM CPU-rate regions are broadly source-aligned. The
  SAM advance sheet slows `$0000-$7FFF` and `$FF00-$FF1F` in address-dependent
  mode; `SamCycleTiming::is_ram_or_io0` currently implements RAM plus IO0 as
  the slow region.
- SAM memory map type and page-select behavior is now source-aligned at the
  board-memory level. In map type 0, P selects which 32 KiB RAM page appears at
  `$0000-$7FFF`. In map type 1, MPU reads/writes below `$FF00` use contiguous
  RAM while the `$FFxx` device/vector page remains decoded.
- Dragon 64 cold boot is modeled as Dragon 32-compatible reset mode with the
  extra Dragon 64 ACIA decode at `$FF04-$FF07`. The native `EXEC 48000`
  transition is modeled through PIA1 PB2 ROMSEL: the selected internal BASIC ROM
  appears at `$8000-$BFFF`, and SAM TY exposes RAM below the `$FFxx`
  device/vector page.
- The vertical active display shape is source-aligned at the practical level.
  The MC6847 documentation describes 192 display lines offset 25 lines from the
  top of the visible VDG picture. `motorola-vdg-6847` uses a 256x192 active text
  framebuffer and a 25-line top border.
- Current `motorola-6809` instruction tests pass and cover real ROM-loader
  paths, arithmetic flags, stack frames, indexed addressing, branch timing,
  `RTI`, `SYNC`, `CWAI`, external interrupt entry, and enough of the official
  opcode map to avoid accidental illegal-opcode traps during normal documented
  execution.

## Known Accuracy Gaps

### MC6809E Pin And Phase Model

The current `Mc6809` API exposes a bus-cycle view:

- `addr`
- `data`
- `data_in`
- `rw`
- `sync`
- interrupt input levels

The primary sources describe a richer MC6809E interface:

- `E` and `Q` are separate clocks, with `Q` leading `E`.
- Addresses become valid after the falling edge of `E`.
- Read data is latched on the falling edge of `E`.
- `NMI`, `FIRQ`, and `IRQ` are sampled on falling edge `Q`, then synchronized
  before service.
- `BA`, `BS`, `BUSY`, `AVMA`, `LIC`, `TSC`, `HALT`, and related bus-state pins
  are externally visible and have behavioral meaning.

The CPU core now has an additive phase-visible API. The compatibility `tick()`
path still advances one bus-visible CPU cycle, while Dragon machine stepping
uses `tick_phase()` around SAM master-tick windows. This is enough to put
interrupt sampling and read-data latching at the documented falling-Q and
falling-E points. Tests now cover falling-Q interrupt sampling during normal
execution, `SYNC`, and `CWAI`, plus the one-cycle synchronization delay before
service. Pin tests now cover reset vector fetches, normal opcode fetch/internal
cycles, `SYNC` acknowledge, diagnostic halt acknowledge, software and hardware
interrupt vector fetches, vector-fetch `BUSY`, `AVMA`, `BA/BS`, and represented
`LIC` states. Internal/non-access cycles now expose the documented `$FFFF`
dummy read in the compatibility pin model. External `HALT` now stops at an
instruction boundary, acknowledges with `BA/BS = 1/1`, and inserts the documented
bus-reacquire dead cycle after release. External `TSC` now three-states the CPU
address/data/`R/W` buffers without asserting `BA`, and Dragon machine stepping
skips CPU memory transactions while those buffers are not driven. Table-driven
pin-trace tests now walk every transcribed documented opcode row and assert that
the core fetches exactly the documented instruction bytes before operand/effective
memory cycles, with no stray PC-window reads from internal cycles; dummy internal
cycles are pinned to the documented `$FFFF`, read, normal-bus, non-`BUSY`,
non-`LIC`, non-`SYNC` shape. It is still not a complete external bus model for a
separate DMA master.

Required resolution:

1. Add a real external DMA-master harness around `TSC` if/when a Dragon
   expansion needs it.

### MC6809 Opcode Validation

The CPU has useful unit coverage, but it is not yet an exhaustive validation
suite against the Motorola tables. Table-driven timing fixtures now cover
branches, explicit stack operations, accumulator and memory RMW families, the
8-bit ALU matrix, indexed `LDA`/`LEA` postbyte additions, and broad base opcode
families. Directed tests now pin `JMP/JSR` indexed timing, indexed-indirect
subroutine/vector cases, `RTI` 6/15-cycle paths, `CWAI` 20-cycle wait entry,
`SYNC` 4-cycle wait entry, IRQ/FIRQ interrupt entry timing, documented
primary/`$10`/`$11` opcode-page slots versus unused diagnostic traps,
source-backed byte-count/cycle-count metadata for the documented opcode pages,
and table-driven pin traces over every transcribed opcode row. The metadata tests
now also exercise variable-cycle rows for conditional long branches, `SYNC`,
`CWAI`, `RTI`, and indexed postbyte additions across every documented indexed
opcode family.

Required resolution:

1. Keep the transcribed official opcode, addressing-mode, byte-count, and
   cycle-count metadata aligned with the Motorola tables; replace it with a
   generated fixture only if we later extract cleaner machine-readable tables.
2. Extend indexed postbyte timing coverage beyond the current `LDA`, `LEA`,
   `JMP`, and `JSR` representatives if a new instruction family proves it has
   distinct bus-cycle behavior.
3. Keep the current diagnostic illegal-opcode trap for development builds, but
   document it as a diagnostic shortcut. The source says unused opcodes are
   undefined and illegal; a clean emulator halt is not hardware behavior.

### SAM Display Offset Timing

`motorola-sam-6883` updates display offset latches immediately. Dragon machine
state now keeps a separate VDG-effective display base that is copied from the
SAM latches only when frame sync falls low. This follows the SAM advance sheet
description that `F6-F0` display-offset bits take effect during the TV vertical
synchronization pulse, when MC6847 `FS` is low.

Implemented behavior:

1. Immediate writes remain visible through the SAM latch state for snapshot/debug
   visibility.
2. Rendering, VDG samples, and `dragon.video.display_base` use the VDG-effective
   base, not the immediate SAM latch.
3. Tests cover writes before the frame-sync-low boundary and writes after the
   boundary that must wait for the next frame-sync fall.

### VDG Horizontal And Fetch Timing

Current VDG vertical shape is defensible. Byte-fetch lead time is source-backed,
and horizontal placement now has an explicit source-vs-crop split. The MC6847
text describes the active display window relative to the blanking-to-blanking
screen span; our runtime framebuffer remains the existing cropped visible frame.

- Source MC6847 blanking-to-blanking span: 193.1 clocks, rounded to 386
  half-pixels.
- Source MC6847 active offset: 28.3 clocks, rounded to 57 half-pixels.
- Source MC6847 active width: 128 clocks, equal to 256 half-pixels.
- Source MC6847 right border from those rounded values: 73 half-pixels.
- Runtime crop: 372 half-pixels wide, with active display at x=60 and a
  56-half-pixel right border. This is deliberately documented as a crop, not the
  raw MC6847 blanking-to-blanking span.

The MC6847 documentation states the display window timing, the 192-line active
height, the 25-line top offset, and that display memory data must be stable four
or eight clock periods before the horizontal display window depending on mode.
The implementation now uses those four/eight VDG-clock requirements for
short-cycle and long-cycle fetch timing.

Required resolution:

1. Decide whether we want to expose a separate raw MC6847 blanking-to-blanking
   framebuffer in addition to the current cropped runtime framebuffer.
2. Keep XRoar comparisons as regression smoke only, not as the timing authority.

### PIA And Analogue Paths

The PIA register model and Dragon keyboard/joystick wiring are good enough for
current usability, and the MC6821 interrupt/strobe coverage now includes
source-backed Cx1/Cx2 edge and strobe behavior. Analogue behavior is still
incomplete:

- Audio uses measured/reference gains and offsets for the documented DAC, tape,
  cartridge `SND`, and single-bit paths, but not full analogue filtering. The
  local Backgammon smoke now asserts active 48 kHz mono output with multiple
  sample levels and sustained transitions, so runtime audio can no longer
  regress to silence or DC without tripping the verifier.
- Native host gamepad left-stick input now reaches the Dragon analogue
  comparator path continuously; headless smoke can now inject explicit
  normalized analogue axis values and sweep them across a range.
- The archived `JOY TEST` CAS now serves as the first regular local-smoke
  comparator fixture: it loads through the ROM, idles stably, and reports
  visible changes across X/Y analogue sweep points.
- Cartridge expansion hardware beyond the documented `SND` input pin is not
  implemented. The fourth Dragon sound mux selection is unused and silent.

Required resolution:

1. Add a synthetic comparator fixture only if archived `JOY TEST` media
   availability makes deterministic assertions awkward.
2. Add analogue filtering and expansion-device audio only after core timing is
   stable and after we have source material for the Dragon analogue output
   stage or specific expansion hardware.

## Validation Plan

1. Stabilize `motorola-6809` as a source-backed CPU core: table-driven opcode
   timing, pin-state traces, interrupt sampling tests, and RMW/vector `BUSY`
   behavior.
2. Re-run Dragon CAS and deterministic PAK trace-signature smoke after each
   timing change, treating XRoar screenshots as advisory regression artifacts
   rather than proof of accuracy.
3. Only after CPU/SAM/VDG timing is source-backed, revisit audio filtering,
   cartridge expansion devices, deeper Dragon 64 64-mode software coverage, and
   disk hardware.

## Immediate Next Engineering Step

Finish the remaining MC6809 source-backed validation before moving back to
analogue/audio. CPU phase stepping, interrupt wait states, SAM display-offset
timing, VDG fetch-to-display timing, VDG source-vs-crop horizontal geometry, and
PIA edge/strobe behavior, opcode metadata, and documented-row bus traces are now
tested. The next CPU gap is only needed if we emulate a real external DMA master
that asserts `TSC`; otherwise the remaining Dragon work is outside the core CPU
accuracy path.

Progress:

- `motorola-6809` now exposes a compatibility `Mc6809Pins` snapshot for the
  bus-state pins we can represent before the full E/Q phase model lands.
- A source-backed opcode timing fixture now checks a first transcribed subset of
  the Motorola timing table.
- That fixture corrected three previous timing shortcuts: `LBRA`, extended
  `JMP`, and `ORCC`/`ANDCC`.
- The timing fixture now covers more base opcode families plus the documented
  indexed `LDA` postbyte cycle additions, including indirect and PCR forms.
- Pin tests now assert `BUSY` and interrupt/vector acknowledge behavior for the
  bus-cycle states the current compatibility model can expose.
- `motorola-6809` now has an additive E/Q phase-visible stepping API. The
  compatibility `tick()` path remains intact, while `tick_phase()` samples
  interrupt inputs on falling Q and latches read data on falling E.
- `machine-dragon-32` now drives the CPU through the phase-visible API for each
  public bus cycle, while preserving the existing SAM master-tick accounting.
- Dragon bus-cycle stepping now splits each SAM CPU cycle into Q-high, E-high,
  Q-low, and E-low master-tick windows. Video, cassette, PIA sync lines, and
  diagnostic VDG traces advance through those windows; CPU memory data is
  supplied at the falling-E point instead of after a whole-cycle video lump.
- MC6809 timing tests now pin `RTI` short/full-frame timing, `SYNC` and `CWAI`
  wait-entry timing, falling-Q interrupt sampling during those wait states,
  external IRQ/FIRQ entry timing, and indexed `JMP/JSR` direct and indirect
  timing. These tests corrected prior shortcuts in full-frame `RTI`, `SYNC`,
  `CWAI`, external interrupt entry, and indexed `JMP`.
- MC6809 opcode-page tests now scan every primary, `$10`, and `$11` slot,
  asserting that documented slots dispatch and unused Motorola table slots hit
  the current diagnostic illegal-opcode trap.
- MC6809 opcode metadata now transcribes documented byte-count shapes and
  fixed/base cycle counts by instruction family, and tests prove that metadata
  covers each documented opcode page exactly once.
- MC6809 metadata tests now execute variable-cycle rows: conditional long
  branches in taken/not-taken form, `SYNC`/`CWAI` wait-entry timing, `RTI`
  short/full-frame timing, and indexed addressing additions across the indexed
  opcode families.
- MC6809 pin tests now pin reset/opcode/vector fetch state, represented
  `LIC`, `AVMA`, `BA/BS`, and `BUSY` behavior for the compatibility pin model.
  The pin snapshot corrected reset high-vector `BUSY`, `LIC` during represented
  `SYNC`/halt acknowledge states, and `$FFFF` dummy reads during internal
  non-access cycles.
- External MC6809 ownership tests now pin logical `HALT` input behavior,
  bus-reacquire dead-cycle behavior after `HALT` release, and `TSC`
  three-state behavior. Dragon machine stepping now uses the CPU pin-drive
  snapshot so high-impedance CPU cycles do not perform synthetic memory
  transactions.
- MC6809 documented-row bus trace tests now assert exact instruction-byte fetch
  windows for every transcribed official opcode row and pin `$FFFF` internal
  dummy cycles to the documented read/normal-bus/non-`BUSY` shape.
- Dragon rendering now separates immediate SAM display-offset latches from the
  VDG-effective display base. `dragon.sam.display_offset` changes immediately,
  while VDG fetches and `dragon.video.display_base` update on frame-sync fall.
- Dragon RAM mapping now follows the SAM P/TY map controls: map type 0 pages
  the low 32 KiB RAM window, and map type 1 exposes contiguous RAM below the
  `$FFxx` device/vector page. This fixes the prior mismatch where TY affected
  cycle timing but not actual memory reads/writes.
- Dragon 64 now has a distinct PAL runtime profile that cold-boots from the
  Dragon 64 compatible BASIC ROM, decodes the Dragon 64 6551 ACIA range at
  `$FF04-$FF07`, enters 64-mode BASIC through `EXEC 48000`, and then accepts
  and evaluates a post-transition BASIC command. Full RS-232 behavior remains
  pending.
- Dragon 64 firmware construction now rejects obviously bad ROM pairs:
  identical compatible/mode images and catalogued CRCs supplied in the wrong
  slot. Unknown 16 KiB alternates are still accepted so valid uncatalogued dumps
  do not fail solely because they are not in our local reference set.
- VDG byte fetch lead time is now mode-aware and source-derived: short-cycle
  modes latch four VDG clocks before display, while long-cycle modes latch
  eight VDG clocks before display. Beam tests now cover writes just before and
  just after the latch point.
- `motorola-vdg-6847` now exposes source-derived MC6847 horizontal geometry
  constants separately from the runtime crop constants, with tests pinning both
  the raw 386-half-pixel span and the existing 372-half-pixel crop.
- `motorola-pia-6821` now models MC6821 Cx2 output strobe modes that restore on
  the next configured Cx1 active edge. Tests cover CA2 read strobe with CA1
  restore, CB2 write strobe with CB1 restore, and stale strobe-state clearing
  when Cx2 returns to input mode.
- Dragon joystick comparator tests now pin the PIA1 PA2-PA7 DAC threshold,
  ignored PA0/PA1 bits, and the exact `axis >= threshold` edge.
- Dragon audio tests now pin PIA1 CB2 mux enable and the four PIA0 CA2/CB2 mux
  source selections: DAC, cassette tape, cartridge `SND`, and the unused mux
  input.
- Dragon audio tests now pin normalized cartridge `SND` input clamping and
  verify that the cartridge input reaches the runtime audio stream only when the
  mux selects it.
- DragonDOS `.BIN` program parsing is now isolated in `format-dragon-bin` and
  pinned to the locally referenced XRoar-compatible header shape. Runtime,
  native, and headless Dragon paths can mount those files as direct program
  media, boot the ROM to the BASIC `OK` prompt, inject the payload into RAM, set
  the BASIC `EXEC` vector, and autorun from the declared exec address. The
  boot-before-EXEC step is required: starting at the entry point from reset-time
  state leaves the hardware stack and display state invalid for real programs.
  `emu198x-dragon --bin-smoke-root` now turns DragonDOS `.BIN` trees into
  structured parse/runtime/screenshot regression reports. The script crate also
  has a synthetic real-ROM `.BIN` smoke regression, so the EXEC path is covered
  without committing archived program binaries. `emu198x-dragon --model
  dragon64 --rom ... --rom64 ...` now routes CAS, `.BIN`, and PAK smoke through
  the Dragon 64 runtime profile. Dragon 32 `.BIN` and PAK smoke intentionally
  keep the older trace-rich harness for now so existing deterministic trace
  signatures remain stable; direct low-level and XRoar-reference modes still
  reject Dragon 64.
- The shared shell input surface now has analogue axis events, and the native
  Dragon shell maps left-stick X/Y to continuous Dragon joystick 1 comparator
  values while preserving D-pad/button digital controls.
- `emu198x-dragon` now exposes `--smoke-joystick-axis` and
  `--smoke-joystick-axis-sweep`, so headless CAS smoke can drive continuous
  comparator values without a physical gamepad and record which sweep points
  visibly affect software output.
- The archived `JOY TEST` CAS loads and runs under the ROM, remains visually
  stable after start, and then visibly reacts to X/Y analogue comparator sweep
  points from the headless smoke harness.
- PAK smoke now emits deterministic `trace_signature` values over retained CPU
  fetches, VDG samples, VDG mode writes, video phase, text, and framebuffer
  data. The local verifier runs a curated Skramble, Doodle Bug, and Hunchback
  set twice and compares those signatures as the stable PAK alignment gate.
  The gate also pins expected `running-visible` classification, minimum colour
  counts, required VDG mode writes where expected, and known-good hashes.
