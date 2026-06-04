# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/mos-via-6522-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- fix workspace clippy and test hygiene
- Run rustfmt across the workspace
- VIA 6522 shift register all 7 modes + external CB1 driver
- VIA 6522 ORA-alt + IER bit 7; SID envelope gate-bug
- Tighten 1541 IEC behavior and trace helpers
- Fix VIA IFR reads and 1541 track-zero sense
- Add first C64 BASIC disk autoload path
- Wire first C64 and 1541 IEC bus state
- Format VIA and 1541 sources
- Add VIA and 1541 drive substrate
