# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- First release under the `emu198x-commodore-agnus-aga` name. Until now the crate was
  `commodore-agnus-aga`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

### Added

- Add regression coverage showing that Alice inherits the clean-idle
  register-equal DDF transition and issues bitplane DMA

### Fixed

- Report PAL/NTSC Alice identity directly from AGA constructors and delay
  D-channel blitter finish until the serialized final-D stage
- Inherit the enhanced `$D8` bitplane-DMA stop and horizontal hard-limit
  bypass policy through Alice's ECS timing layer
- Preserve Alice's observed ordinary DDFSTOP and frozen current-line
  fetch endpoint across later register writes and snapshots
- Keep Alice wide-fetch arbitration aligned to the DDFSTRT comparator
  that started the current line, even after the register is rewritten
- Inherit ECS programmable vertical-blank timing for Alice sprite DMA
- Inherit the enhanced ten-bit sprite vertical comparators
- Limit Alice `DIWHIGH` vertical extensions to V10..V8 while retaining
  the ECS Agnus V11 extension on ECS machines

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-agnus-aga-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- A1200 Stage A: AGA chipset + Gayle + machine scaffold
