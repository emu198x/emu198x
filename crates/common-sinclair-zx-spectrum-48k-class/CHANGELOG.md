# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/common-sinclair-zx-spectrum-48k-class-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- port_read / port_write — direct bus-level Z80 I/O
- full-trace memory_read / poke / watch_memory tools
- cargo fmt --all across the workspace
- Open Emu198x for public release
- 5 new constructor + ROM-size tests
- Boot invariants: 5 new Seam 5 waypoint assertions for the 48K runtime
- Rename BoardIssue → UlaRevision with explicit revision variants
- pinpoint wipe trigger at $fd6c (L=$28, want $3A)
- +3 disk Loader now runs end-to-end (architecturally)
- Reattach Spectrum ULA timing config on snapshot restore
- Wire portable .sna / .z80 snapshot import; rename State menu honestly
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Fix new clippy lints introduced by Rust 1.95.0
- Lift Kempston joystick to a Peripheral, migrate all hosting machines
- Complete Spectrum SOLID variant coverage via class layer crates
