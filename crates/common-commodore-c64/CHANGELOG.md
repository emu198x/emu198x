# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-c64-v0.2.0) - 2026-06-04

### Added

- *(c64)* light up the disasm debug surface (wire a 6502 DebugTarget)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Seam 3 (NES + C64): serde-skip audit lock + real PAL APU fidelity fix
- C64 Seam 1: VIC-II BA/RDY audit landed
- add native channel controls
- Update test fixtures for 7-cycle 6502 reset + expose reset_phase
- Format C64 files after workspace fmt
- Add C64 joystick input and Bruce Lee disk proofs
- Fix C64 IEC output polarity for 1541 loads
- Fix VIA IFR reads and 1541 track-zero sense
- Wire first C64 and 1541 IEC bus state
- Fix C64 6510 banking and Ghostbusters tape flow
- Add C64 tape trace query surface
- Tighten C64 datasette state and tape queries
- Add C64 datasette TAP path
- Fix C64 keyboard matrix scan orientation
- Wire live SID into C64 runtime
- Add C64 host-side program import
- Add C64 snapshots and headless runner
- Add C64 runtime boot detection and frame output
- Add MOS VIC-II and wire it into C64
- Add MOS 6526 CIA and wire it into C64
- Add MOS 6502 core and C64 CPU loop
- Add C64 machine substrate crate
