# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose the complete common Denise register, bitplane, collision, sprite,
  HAM and wide-fetch pipeline through grouped diagnostics
- Expose Paula's interrupt priority, audio pipelines and controls, UART,
  pot-port pins and component logs through grouped diagnostics
- Expose controller-port sources, keyboard protocol progress and all existing
  machine diagnostic counters and last-entry summaries through grouped queries
- Resolve and advertise nested query fields, with recursive catalogue tests
  covering every object field returned by each diagnostic group
- Expose runtime buffering, CPU-domain scheduling, common Copper pipelines,
  complete floppy mechanism/track-stream/Paula disk state and Copper MOVE-log
  summaries through discoverable grouped queries
- Lift the common Copper and its bounded MOVE log through `AmigaLiveAccess`
  on OCS, ECS and AGA machines
- Expose complete ECS/AGA programmable timing, decoded selector, sync-pin and
  composed horizontal-blank state through discoverable grouped and leaf
  queries
- Advertise every existing grouped Amiga diagnostic field and standalone
  sprite query, with catalogue tests that reject future discovery drift
- Distinguish the floppy mechanism's motor-at-speed state from the
  multiplexed active-low READY pin in disk diagnostics
- Add an explicit Amiga Test Kit v1.21 video-conformance lane for A500+A501
  OCS PAL reference-pattern capture and independently sourced comparison
- Add an explicit, checksum-pinned Amiga Test Kit v1.12 gate covering stock
  A500 and GVP A530 guest identification, visible menu/input progress, and
  deterministic A530 snapshot replay
- Add canonical immutable Amiga processor, accelerator, and machine
  configurations, including PAL and NTSC A500 + GVP A530 research profiles
- Preserve every higher-CPU instruction boundary for tracing and stop shared
  debugger steps after exactly one instruction
- Preserve MC68020 logical data transfers across sized A1200 chip-RAM phases,
  including unaligned longword accesses
- Refresh the A1200 Kickstart 3.1 and Workbench 3.1 golden frame for the
  corrected horizontal-scroll arrow glyphs produced by line-mode blits
- Expose Copper busy, completion phase, remaining completion CCKs and
  final-D pending state through Agnus and blitter query namespaces
- Expose internal busy, visible busy and remaining startup CCKs as distinct
  Agnus and blitter query leaves

### Changed

- [breaking] Bump Amiga postcard snapshots to version 28 so ECS Agnus and
  AGA Lisa programmable horizontal-blank event history survives restore;
  version 27 is rejected because it cannot preserve those hidden latches
- [breaking] Bump Amiga postcard snapshots to version 27 so Denise's
  prior-line raster carry, raw field identity and HBLANK-reset context survive
  restore; version 26 is rejected because it cannot preserve that context
- [breaking] Bump Amiga postcard snapshots to version 26 so Denise's per-line
  BPL1DAT sprite-visibility latch survives restore; version 25 is rejected
  because it cannot preserve per-line sprite visibility
- Timestamp public CPU-trace entries with the zero-based Amiga system tick
  containing the boundary; several higher-CPU boundaries can share one tick
- Classify the existing unvalidated A500, A500+A501, A500-maxed, and A1200
  NTSC profiles as Research rather than Boots
- [breaking] Extend the public exhaustive `Model`, `CpuKind`, and
  `Accelerator` enums with the A500/A530 profiles and the higher-CPU
  configuration vocabulary
- [breaking] Extend the public `AmigaMachine` implementation surface with
  canonical construction, exact CPU-boundary advancement, boundary draining,
  and restored-configuration validation
- [breaking] Bump Amiga postcard snapshots to version 25 so original-Agnus
  identity is validated against the configured machine and region; version 24
  is rejected because it could persist reversed PAL/NTSC identity bits
- Bump Amiga postcard snapshots to version 24 and persist the canonical
  construction configuration alongside ActiveCpu and CPU-clock machine state;
  version 23 is rejected
- Bump Amiga postcard snapshots to version 23 so an in-flight
  MC68020/MC68030 dynamic-sized transfer retains its remaining SIZ value,
  complete write operand, partial read accumulator and current bus outputs;
  version 22 is rejected
- Bump Amiga postcard snapshots to version 22 so MC68020+ master-mode
  interrupt entry and Format-$1 RTE retain their pending phase, buffered
  SR/PC and selected USP/ISP/MSP bank; in-flight UNLK state now carries
  explicit stack-bank identity too. Format-$A entry/RTE phases are retained,
  and version 21 is rejected
- Bump Amiga postcard snapshots to version 21 so a wrapped MC68010-or-later
  CPU retains its pending frame PC and can resume between interrupt
  acknowledge and Format/Vector frame construction; version 20 is rejected
- Bump Amiga postcard snapshots to version 20 so the MC68000's sampled
  interrupt level and pending lower-to-level-7 transition survive restore;
  version 19 is rejected
- Bump Amiga postcard snapshots to version 19 because a version-18
  snapshot taken during interrupt acknowledge can retain the old fixed
  level-7 address, a pre-acceptance active SR mask or an autovector
  derived from mutable live IPL
- Bump Amiga postcard snapshots to version 18 so active line-mode ONEDOT
  row eligibility, B texture phase and current-CCK nasty ownership validity
  survive restore; version 17 is rejected
- Bump Amiga postcard snapshots to version 17 so pre-AGA and Alice
  completion phases, observer holds, one-shot finish state and same-CCK
  blitter bus use survive restore; version 16 is rejected
- Bump Amiga postcard snapshots to version 16 so the shared two-CCK
  blitter-startup phase and pending Copper `WAIT`/`SKIP` kind survive restore;
  version 15 is rejected because it cannot preserve A1000 BBUSY visibility,
  BZERO reload timing, admission of the first channel operation or a deferred
  `SKIP` decision
- Bump Amiga postcard snapshots to version 15 so installed original-Agnus
  revision identity and its line-held hard vertical-blank force-off state
  survive restore; version 14 is rejected because it cannot distinguish
  A1000 line-zero timing from the later final-line close
- Bump Amiga postcard snapshots to version 14 so the MC68000's
  group-0/group-1 exception-processing history survives restore;
  version 13 is rejected because it cannot reconstruct address-error
  I/N context or recursive-fault handling
- Bump Amiga postcard snapshots to version 13 so the hidden
  original-Agnus vertical display-window latch and resulting DDF abort
  history survive restore; version 12 is rejected
- Bump Amiga postcard snapshots to version 12 because version 11 can
  preserve an original-Agnus abort after the old runtime missed a
  rewritten future DDFSTRT comparator; version 11 is rejected
- Bump Amiga postcard snapshots to version 11 so an original-Agnus
  run aborted by bitplane DMA disable cannot resume after restore;
  version 10 is rejected
- Bump Amiga postcard snapshots to version 10 because version 9 can
  preserve an open original-Agnus start gate after discarding a
  phase-shifted `$E3` terminal endpoint at short-line wrap
- Bump Amiga postcard snapshots to version 9 so the serialized
  original-Agnus horizontal hard-start gate survives line boundaries;
  version 8 is rejected
- Bump Amiga postcard snapshots to version 8 because a version-7
  snapshot can restore an active enhanced-chipset fetch region after
  `$D8` without the newly required terminal endpoint; version 7 is
  rejected
- Bump Amiga postcard snapshots to version 7 because a version-6 snapshot
  can restore an active original-Agnus fetch region after `$D8` without
  a terminal endpoint and continue past the fixed boundary; version 6 is
  rejected
- Bump Amiga postcard snapshots to version 6 so the current-line
  DDFSTOP comparator match and frozen final fetch endpoint survive save
  and restore; version 5 is rejected
- Bump Amiga postcard snapshots to version 5 so the current-line
  DDFSTRT comparator match and frozen fetch origin survive save and
  restore; version 4 is rejected
- Bump Amiga postcard snapshots to version 4 so the hidden ECS/AGA
  vertical display-window latch survives save and restore; version 3
  is rejected
- Bump Amiga postcard snapshots to version 3 so OCS-shaped machine
  snapshots preserve whether early Agnus or Fat Agnus 8372A is installed,
  including the latter's ECS extension-register state; version 2 is rejected
- Bump Amiga postcard snapshots to version 2 for serialized programmable
  vertical-blank and current-CCK sprite-arbitration state; version 1 is rejected

### Fixed

- Publish fixed-sync PAL and NTSC fields only after Denise retires the
  post-wrap raster tail, keeping the final framebuffer rows current through
  the right edge
- Correct the public A1000/Fatter-Agnus chip-RAM ceiling to its addressable
  512 KiB while retaining the 256 KiB shipping profile
- Validate model-specific Agnus RAM ceilings before construction
- Reject malformed persisted floppy images and incoherent A530 state before
  committing a snapshot restore, and clear observational CPU traces only
  after a successful restore
- Reject snapshot states that advance a downstream Autoconfig board before
  the A530 leaves the probe window or map two configured boards over the same
  address range
- Reject non-canonical audio resampler phases and chipset framebuffers whose
  serialized length does not exactly match the selected machine
- Reset transient Paula analog-filter history after machine reset and
  successful snapshot restore
- Preserve partially consumed higher-CPU system ticks across snapshots
- Preserve ECS and A1200 NTSC region selection across runtime reset

- Preserve OCS-shaped A2000 and maxed-A500 profiles while selecting
  their installed Fat Agnus 8372A identity, RAM ceiling, sprite
  comparators, programmable timing and large-blit registers explicitly,
  independently from RAM size and OCS Denise
- Boot Kickstart and Workbench 2.04 through the mixed A2000B chip stack,
  including V36 extended-blitter writes, without upgrading OCS Denise

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-commodore-amiga-v0.2.0) - 2026-06-04

### Fixed

- *(input)* [**breaking**] number joystick ports by the documented hardware labels

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(amiga)* joystick lifecycle test uses control port 2
- retire emu198x-script-* references after binary consolidation
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-h + AE-i: investigation tooling — chipset write log + CPU instruction trace
- A1200 Stage AE-f: rename AmigaA1200Session → AmigaSession
- A1200 Stage AE-e: mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
- A1200 Stage AE-c: route cross-cutting MCP tools through AmigaLiveAccess
- A1200 Stage AE-b: AmigaLiveAccess trait — chipset-agnostic chip access
- catalogue fills — A600 ECS + A2000 OCS
- A1200 MCP session migrates to family runtime
- A1200 impls AmigaMachine + joins AmigaRuntimeKind::Aga
- hierarchical amiga_model catalogue + ChipsetKind / CpuKind
- Open Emu198x for public release
- Bump png 0.17 → 0.18
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Fix floppy spin-up regression that broke KS 2.04 disk loading
- Phase 0 closed: WB 2.04 desktop + verifier-binary dispatch
- KS 2.04 boots A500+ ECS to insert-disk in ~50M ticks
- KS 2.04 boot probe: stalls on both OCS and ECS chip stacks
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
- Amiga NTSC: chip-layer line alternation + 5 NTSC OCS variants
- Convert runtime-commodore-amiga to AmigaRuntime<M: AmigaMachine>
- Post-track tidy: rustfmt sweep + motorola-68000 doc accuracy
- directed-test passes across the runtime family
- Split Amiga runtime into queries / snapshot / input modules
- Add boot_invariants.rs for the four anchor families
- Add Amiga postcard snapshots across the chip stack
- add amiga joystick controls
- add native channel controls
- wire native mouse input
- drain Paula audio through runtime
- Apply mechanical Rust formatting cleanup
- normalize family profile layout and game boy contract
- resolve remaining diagnostic cleanup
- commit mechanical cleanup across diagnostics
- fix ks12 slow-ram probe and add a1000 goldens
- add real a1000 bootstrap WOM support
- add wb13 desktop golden
- Run rustfmt across the workspace
- land wb13 boot investigation and fixes
- WB 1.3 diag: dump every blit that lands in the bootblock buffer
- WB 1.3 diag: chase the validation/extraction path through the ROM
- WB 1.3 diag: trace validation result + cksum-verify across syncs
- fix ADKCON bit constants — WORDSYNC was silently the wrong bit
- WB 1.3 diag: pinpoint the silent failure — chained QBlits never run
- WB 1.3 diag: count byte-aligned \$AAAA and \$5555 gap-words
- WB 1.3 diag: rule out bit-level false-sync theory
- WB 1.3 diag: verify chip RAM matches encoder byte-for-byte
- Phase A: narrow WB 1.3 boot hang to MFM format compat with trackdisk
- Complete disk DMA transfers so trackdisk's DSKBLK fires
- Recapture Amiga golden matrix PNGs from current OCS output
- paint COLOR00 through the full viewport for the border
- Fix DMACONR byte-read: upper byte on even addresses
- Narrow OCS insert-disk regression to LoadView path + COP2LC thrash
- Pin OCS insert-disk regression to KS 1.3 clearing BPLEN + SPREN
- Golden matrix: compare against FS-UAE references at PAL-cropped 752×572
- Amiga boot-path golden-image matrix (phase 1, OCS)
- Configurable RAM + fast-RAM autoconfig: runtime presets (step 3 of 3)
- drop the slow-RAM forced default for A500 (task #182)
- Port emu198x-script-amiga: retire archive, restore boot.* queries
- Port Amiga runtime: target machine-commodore-amiga-ocs
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Correct freetwice_trace K1.3 addresses — AN_FreeTwice is NOT firing
- Trace writes to corrupted free-list slot — list-node reuse confirmed
- Add freetwice_trace diagnostic — confirms free-list corruption
- Add bootblock_writers diagnostic identifying corruption sources
- Add CPU write-watch instrumentation to find buffer overwriters
- Add MFM + bootblock + DMA-buffer verification examples
- Add boot-debug examples that exposed the CIA double-read bug
- Extend signal_watch with CIA-B/EXTER/PORTS instrumentation
- Add strap-to-trackdisk boot-path diagnostic examples
- CIA 8520 8520-specific TOD halt + floppy /DSKRDY ID stream
- Add screen capture + trackdisk LVO probe, widen trace ranges
- Add disk boot trace examples pinpointing strap hang in CMD_READ
- Add memory watch_range diagnostic + ViewLord trace example
- Add Phase 0/3/5/9 boot-invariant checks against the reference
- Add Amiga boot diagnostics that pin the green/yellow screen root cause
- Correct Amiga CIA TOD alarm semantics and floppy status reporting
- Restore Amiga Kickstart insert-disk screen
- Add Amiga boot diagnostics and CIA TOD fix
- Tighten Amiga CIA and floppy boot path
- Add fresh Amiga headless baseline
