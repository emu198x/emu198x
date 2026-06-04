# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/common-sinclair-zx-spectrum-128k-class-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- watch_ay_* — AY register-write tracer
- port_read / port_write — direct bus-level Z80 I/O
- full-trace memory_read / poke / watch_memory tools
- cargo fmt --all across the workspace
- Open Emu198x for public release
- 7 new io_read / io_write / reset / audio tests
- Boot invariants: 4 new 128K-family waypoints (Seam 5 expansion)
- Kempston joystick input routing (Seam 2)
- AY R14 / R15: model the Sinclair 128K port-A pull (0xBF)
- Mix AY chip output into the 128K-family speaker
- +3 disk Loader now runs end-to-end (architecturally)
- Reattach Spectrum ULA timing config on snapshot restore
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Fix new clippy lints introduced by Rust 1.95.0
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Lift Kempston joystick to a Peripheral, migrate all hosting machines
- Complete Spectrum SOLID variant coverage via class layer crates
