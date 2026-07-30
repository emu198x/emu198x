# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add regression coverage for the non-empty idle register-equal
  DDFSTRT/DDFSTOP transition

### Fixed

- Drive programmable horizontal blanking from serialized HBSTRT/HBSTOP edge
  latches and sample BLANKEN at HBSTRT, so mid-line register writes cannot
  reconstruct an interval behind the beam
- Derive the Copper comparator's horizontal wrap from programmed `HTOTAL`,
  current LOL state and the resulting line-length parity
- Inherit the serialized pre-AGA blitter completion pipeline, including
  early main finish, trailing BZERO/final D and separate busy observers
- Apply the enhanced-chipset `$D8` bitplane-DMA stop by default, while
  `HARDDIS`, `VARBEAMEN`, `SHRES` and `UHRES` disable the horizontal
  hard limit and `VARVBEN` remains vertical-only
- Preserve an observed ordinary DDFSTOP and its pending final fetch unit
  across later register writes and snapshots
- Retain a DDFSTRT comparator match independently of the DMA and
  vertical-window gates, and use its frozen phase for the current line
- Drive ECS bitplane vertical eligibility from a serialized
  VSTART/VSTOP display-window latch, so unreachable `DIWHIGH` starts
  cannot open DMA and comparator writes cannot reconstruct live state
- Treat an explicit `DIWHIGH=$0000` as direct high-bit decoding rather
  than falling back to the legacy implicit VSTOP bit
- Drive sprite blanking and control refetch from the edge-driven
  `VBSTRT`/`VBSTOP` state selected by `BEAMCON0.VARVBEN`
- Keep untouched programmable blank comparators unarmed when other
  vertical-timing registers are written
- Decode the undocumented `SPRxCTL` bit-6/bit-5 VSTART[9]/VSTOP[9]
  extensions for direct and DMA-fetched control words

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-agnus-ecs-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-k: ECS blitter extension registers — WB content draws on ECS
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- Open Emu198x for public release
- Wire AmigaEcs machine + AmigaEcsRuntime; reclassify A500+ as ECS
- Lift commodore-agnus-ecs and commodore-denise-ecs from archive
