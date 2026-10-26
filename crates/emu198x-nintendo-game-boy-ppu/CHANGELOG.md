# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.0

### Added

- `SCREEN_WIDTH` and `SCREEN_HEIGHT`, the LCD's size, which the PPU used to
  import from the Game Boy family's glue crate. The glue now re-exports them
  from here, so the PPU depends on nothing in its family.
- First release under the `emu198x-nintendo-game-boy-ppu` name. Until now the crate was
  `nintendo-game-boy-ppu`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/nintendo-game-boy-ppu-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Add Game Boy skipped boot profiles
- Fix Game Boy sprite mode timing
- Fix Game Boy VBlank STAT timing
- Improve Game Boy PPU access timing
- Improve Game Boy mooneye baseline
- Add nintendo-game-boy-ppu crate — Phase 1 step 4
