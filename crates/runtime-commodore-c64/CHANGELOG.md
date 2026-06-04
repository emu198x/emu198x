# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-commodore-c64-v0.2.0) - 2026-06-04

### Added

- *(c64)* light up the disasm debug surface (wire a 6502 DebugTarget)

### Fixed

- *(input)* [**breaking**] number joystick ports by the documented hardware labels

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* make the debug-target macros storage-agnostic
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Seam 5 (NES + C64): boot invariant suite extensions
- Seam 2 (NES + C64): host input → controller / joystick routing
- Fix new clippy lints introduced by Rust 1.95.0
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Extend Aztec diagnostic with CPU state + mid-frame VIC sample
- Add Aztec player-select VIC state diagnostic
- Post-track tidy: rustfmt sweep + motorola-68000 doc accuracy
- Close Cov-4: directed tests for the isolated C64 runtime modules
- Extract C64 runtime tests into per-topic integration files
- Split C64 runtime into queries/snapshot/input modules
- Add boot_invariants.rs for the four anchor families
- add native channel controls
- normalize family profile layout and game boy contract
- Format C64 files after workspace fmt
- Tighten 1541 IEC behavior and trace helpers
- Add Bomb Jack C64 disk regression
- Add Aztec Challenge C64 disk regression
- Add C64 joystick input and Bruce Lee disk proofs
- Add Bruce Lee C64 disk start proof
- Fix C64 IEC output polarity for 1541 loads
- Improve 1541 byte-ready timing and tracing
- Fix VIA IFR reads and 1541 track-zero sense
- Add first C64 BASIC disk autoload path
- Mount D64 media into live 1541 path
- Attach live 1541 runtime to C64
- Add C64 D64 container import support
- Add Thing on a Spring C64 start interaction proof
- Add Thing on a Spring C64 tape regression
- Add C64 VIC colour-write trace
- Fix C64 6510 banking and Ghostbusters tape flow
- Add C64 tape trace query surface
- Tighten C64 datasette state and tape queries
- Add second C64 tape software regression
- Strengthen C64 Thinker tape regression
- Add C64 tape autoload and T64 import
- Add C64 datasette TAP path
- Add C64 native verifier shell
- Wire live SID into C64 runtime
- Add C64 host-side program import
- Add C64 snapshots and headless runner
- Add C64 runtime boot detection and frame output
- Bootstrap C64 profile and timing crates
