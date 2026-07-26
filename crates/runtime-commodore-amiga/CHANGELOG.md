# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose internal busy, visible busy and remaining startup CCKs as distinct
  Agnus and blitter query leaves

### Changed

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
