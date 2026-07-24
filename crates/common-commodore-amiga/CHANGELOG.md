# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Phase Denise's bitplane pipeline from Agnus's matched DDFSTRT origin
  rather than a mutable register value
- Gate board-level Amiga output with the concrete Agnus or Alice vertical
  display-window state instead of re-decoding legacy OCS bounds

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/common-commodore-amiga-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-a: HIRES dma_claim schedule
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage T: wire AGA registers to the chipset bus
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Amiga Seam 1.7: move copper.rs to common-commodore-amiga
- Amiga Seam 1.6: Denise wrapper goes generic over DeniseChip
- Amiga Seam 1.5: add DeniseChip trait
- Amiga Seam 1.4: move memory.rs into common-commodore-amiga
- Amiga Seam 1.3: move cia.rs into common-commodore-amiga
- Amiga Seam 1.2: move rtc.rs into common-commodore-amiga
- Amiga Seam 1.1: scaffold common-commodore-amiga crate
