# Dragon Accuracy Audit

Last updated: 2026-04-30.

This audit is about our own Dragon 32 emulation accuracy, not matching another
emulator. Other emulators are still useful as smoke references, but hardware and
Motorola/Dragon source material are the authority when behavior differs.

## Primary Sources

- `docs/source-extracts/dragon-primary/mc6809-mc6809e-programming-manual-1981.txt`
- `docs/source-extracts/dragon-primary/mc6809e-hmos-microprocessor-1984.txt`
- `docs/source-extracts/dragon-primary/mc6821-pia-1985.txt`
- `docs/source-extracts/dragon-primary/mc6847-video-display-generator-1984.txt`
- `docs/source-extracts/dragon-primary/mc6883-sam-advance-sheet.txt`
- `docs/source-extracts/dragon-primary/sam-programming-guide.txt`

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
- The vertical active display shape is source-aligned at the practical level.
  The MC6847 documentation describes 192 display lines offset 25 lines from the
  top of the visible VDG picture. `motorola-vdg-6847` uses a 256x192 active text
  framebuffer and a 25-line top border.
- Current `motorola-6809` instruction tests pass and cover real ROM-loader
  paths, arithmetic flags, stack frames, indexed addressing, branch timing, and
  the official opcode map enough to avoid accidental illegal-opcode traps during
  normal documented execution.

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
falling-E points, but it is not yet a complete external bus model for DMA,
halt/three-state ownership, or every invalid-vs-valid memory access cycle.

Required resolution:

1. Expand bus-trace tests for reset, fetch, RMW, double-byte operations, vector
   fetches, `SYNC`, `CWAI`, `HALT`, and interrupt entry.
2. Model the remaining external ownership pins and DMA/three-state behavior
   once a machine needs them.

### MC6809 Opcode Validation

The CPU has useful unit coverage, but it is not yet an exhaustive validation
suite against the Motorola tables. Cycle counts are tested opportunistically
inside instruction tests rather than from a single source-backed opcode matrix.

Required resolution:

1. Transcribe the official opcode, addressing-mode, byte-count, and cycle-count
   tables into test fixtures.
2. Generate table-driven tests for primary, `$10`, and `$11` opcode pages.
3. Cover indexed postbyte timing variants explicitly, because indexed timing is
   where many 6809 compatibility bugs hide.
4. Keep the current diagnostic illegal-opcode trap for development builds, but
   document it as a diagnostic shortcut. The source says unused opcodes are
   undefined and illegal; a clean emulator halt is not hardware behavior.

### SAM Display Offset Timing

`motorola-sam-6883` updates display offset latches immediately, and Dragon video
currently reads `display_base()` directly. The SAM advance sheet states that
`F6-F0` display-offset bits take effect during the TV vertical synchronization
pulse, when MC6847 `FS` is low.

This is a real raster accuracy gap. Software that changes the display base
mid-frame should not necessarily affect the address stream immediately.

Required resolution:

1. Preserve immediate writes to the SAM latch state for snapshot/debug
   visibility.
2. Add a separate VDG-effective display base or display-address preload state.
3. Apply pending `F6-F0` changes only at the frame-sync-low boundary.
4. Add tests that write the display base before, during, and after the `FS` low
   transition and verify which frame sees the new screen.

### VDG Horizontal And Fetch Timing

Current VDG vertical shape is defensible, but horizontal placement and fetch
lead time still need a source-backed derivation. In particular:

- `TEXT_LEFT_BORDER_PIXELS = 60`
- `TEXT_RIGHT_BORDER_PIXELS = 56`
- `VDG_LINE_MASTER_TICKS = 912`
- `VDG_LEFT_BORDER_TICKS = 120`
- `VDG_FETCH_FIRST_BYTE_AFTER_HBLANK_TICKS = 16`
- `VDG_VRAM_FETCH_TO_DISPLAY_TICKS` is currently documented from XRoar
  behavior rather than Motorola source material.

The MC6847 documentation states the display window timing, the 192-line active
height, the 25-line top offset, and that display memory data must be stable four
or eight clock periods before the horizontal display window depending on mode.
We need to turn that into explicit constants and tests.

Required resolution:

1. Re-derive horizontal blank, display-window start, active width, and fetch
   lead time from the MC6847 and SAM timing descriptions.
2. Replace emulator-derived comments with source-backed comments.
3. Add beam tests for writes just before and just after the VDG latches a byte.
4. Keep XRoar comparisons as regression smoke only, not as the timing authority.

### PIA And Analogue Paths

The PIA register model and Dragon keyboard/joystick wiring are good enough for
current usability, but analogue behavior is still incomplete:

- Audio uses measured/reference gains and offsets, but not full analogue
  filtering.
- Host gamepad input is still thresholded before it reaches the Dragon analogue
  comparator path.
- Cartridge audio and AY expansions are not implemented.

Required resolution:

1. Add continuous analogue axis events to the shared input surface.
2. Validate PIA interrupt edge behavior against MC6821 documentation with
   targeted tests.
3. Add analogue filtering and expansion audio only after core timing is stable.

## Validation Plan

1. Stabilize `motorola-6809` as a source-backed CPU core: table-driven opcode
   timing, pin-state traces, interrupt sampling tests, and RMW/vector `BUSY`
   behavior.
2. Add SAM frame-sync display-base timing tests and implementation.
3. Re-derive VDG horizontal timing and byte-latch timing from primary sources,
   then update constants and tests.
4. Re-run Dragon CAS and PAK smoke after each timing change, but treat those as
   integration checks rather than proof of accuracy.
5. Only after CPU/SAM/VDG timing is source-backed, revisit audio filtering,
   cartridge audio, Dragon 64 mode, and disk hardware.

## Immediate Next Engineering Step

Move to SAM display-offset timing. The CPU now has source-backed timing fixtures
and Dragon uses phase-visible stepping, so the next raster accuracy gap is
separating immediate SAM latch writes from the frame-sync-delayed display base
that the VDG actually sees.

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
