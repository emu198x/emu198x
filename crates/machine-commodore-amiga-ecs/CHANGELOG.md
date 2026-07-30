# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- [breaking] Preserve the two programmed horizontal-blank event latches in
  the raw `AmigaEcsSnapshot` postcard schema; runtime envelopes version this
  as V28
- [breaking] Store the stock MC68000 through the active-CPU boundary and
  preserve its exact 1:1 CPU-clock phase in the raw `AmigaEcsSnapshot`
  postcard schema; runtime envelopes version this as V24

### Added

- Retain a bounded, non-snapshot instruction-boundary queue for runtime
  tracing

### Fixed

- Compose rendered programmable horizontal blanking from Agnus's observed
  HBSTRT/HBSTOP events and the live Super Denise selectors
- Route Copper horizontal comparison through the ECS Agnus projection so
  programmed beam timing reaches `WAIT` and `SKIP`
- Reset generic Autoconfig state on CPU RESET without clearing expansion RAM
- Preserve the inherited pre-AGA finish/result/final-D pipeline and drain
  internal completion rather than stopping at the early source event
- Consume the shared two accepted blitter-startup CCKs before the first
  channel operation while retaining immediate enhanced-chip BBUSY visibility
- Apply the enhanced `$D8` bitplane-DMA stop and horizontal hard-limit
  bypass policy through the native ECS machine loop
- Resolve the DDFSTOP comparator before a same-CCK Copper MOVE, so an
  old match wins and a current or past replacement cannot stop retroactively
- Apply Copper DDFSTRT writes only to unreached comparator events,
  preventing writes at or behind the beam from starting the current line
- Preserve the ECS `DIWHIGH` vertical window when gating rendered output

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-amiga-ecs-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-k: ECS blitter extension registers — WB content draws on ECS
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- A1200 Stage AE-h + AE-i: investigation tooling — chipset write log + CPU instruction trace
- A1200 Stage AE-e: mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Amiga Seam 1.7: move copper.rs to common-commodore-amiga
- Amiga Seam 1.6: Denise wrapper goes generic over DeniseChip
- Amiga Seam 1.4: move memory.rs into common-commodore-amiga
- Amiga Seam 1.3: move cia.rs into common-commodore-amiga
- Amiga Seam 1.2: move rtc.rs into common-commodore-amiga
- Apply horizontal DIW gate to Denise output — fixes KS 2.04 wraparound
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
