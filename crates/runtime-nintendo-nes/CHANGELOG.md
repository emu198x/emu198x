# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-nintendo-nes-v0.2.0) - 2026-06-04

### Added

- *(nes)* flesh out MCP server with 11 NES-specific tools
- sub-frame tick step for cycle-exact MCP debugging
- NES PPU-state debugging queries for the blargg/MCP surface

### Fixed

- NES NMI edge detection on an instruction's final cycle

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Seam 5 (NES + C64): boot invariant suite extensions
- Seam 2 (NES + C64): host input → controller / joystick routing
- Update Unclean/Reference asset paths to assets/
- directed-test passes across the runtime family
- Split NES runtime into queries / snapshot / input modules
- Add boot_invariants.rs for the four anchor families
- Add automated NES Blargg assertions
- Add first-pass NES MMC5 mapper support
- Add NES VRC2a and Action 53 mappers
- Expand NES mapper and runtime coverage
- support mapper 34 NINA variant
- add Camerica mapper support
- add BxROM mapper support
- add AxROM mapper support
- add MMC3 mapper support
- add CNROM mapper support
- add MMC1 mapper support
- add native apu channel controls
- normalize family profile layout and game boy contract
- Add fresh NES headless runtime path
