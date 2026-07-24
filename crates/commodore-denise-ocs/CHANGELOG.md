# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Let armed manual sprite data repeat on every line until `SPRxCTL`
  disarms it, leaving VSTART/VSTOP lifecycle decisions to Agnus

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-denise-ocs-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Apply cargo fmt across in-tree edits + refresh Cargo.lock
- Cov-5c wave 2: directed-test passes across five chip crates
- Split Denise into chip / debug / viewport modules
- Add Amiga postcard snapshots across the chip stack
- fix workspace clippy and test hygiene
- land wb13 boot investigation and fixes
- Retire commodore-denise-ocs-archive: the archive is now the live crate
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)
- Lock in chip-only investigation: tests, fixes, golden framework, restart plan
- Add fresh Amiga headless baseline
