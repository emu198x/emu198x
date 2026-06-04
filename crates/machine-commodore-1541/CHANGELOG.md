# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-1541-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Fix more clippy lints from Rust 1.95.0
- Cov-5c wave 2: directed-test passes across five chip crates
- Update test fixtures for 7-cycle 6502 reset + expose reset_phase
- Tighten 1541 IEC behavior and trace helpers
- Tighten 1541 read path timing
- Improve 1541 byte-ready timing and tracing
- Fix VIA IFR reads and 1541 track-zero sense
- Add first C64 BASIC disk autoload path
- Mount D64 media into live 1541 path
- Attach live 1541 runtime to C64
- Wire first C64 and 1541 IEC bus state
- Format VIA and 1541 sources
- Add VIA and 1541 drive substrate
