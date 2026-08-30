# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Advance live-machine snapshots to version 4 for the beeper integration state; older snapshots cannot resume part-way through an output-sample window
- Advance live-machine snapshots to version 3 and rehydrate the Z80 walker sequence before resumed execution; version 2 cannot preserve accepted interrupt response identity

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-jupiter-ace-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(parity)* Jupiter Ace / Acorn Atom / Oric Atmos / SVI-328 operational parity

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
