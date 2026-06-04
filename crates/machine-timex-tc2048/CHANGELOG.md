# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-timex-tc2048-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(timex)* boot golden screenshots for TC2048 and TS2068
- *(spectrum)* boot integration tests for Scorpion / TC2048 / TS2068
- port_read / port_write — direct bus-level Z80 I/O
- Open Emu198x for public release
- Tree housekeeping: project relocation paths + Cargo.lock
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Lift Kempston joystick to a Peripheral, migrate all hosting machines
- Lock Spectrum SOLID criteria; extract SNA and snapshot crates
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Consolidate Spectrum per-machine boilerplate
- Wrap every Spectrum variant in a generic MachineCore runtime
- Factor SpectrumDriver trait + .z80 snapshot helpers across 7 machines
- Add Timex TC2048 + TC2068/TS2068 machines, extend runtime catalogue
