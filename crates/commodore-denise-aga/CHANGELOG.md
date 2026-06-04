# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/commodore-denise-aga-v0.2.0) - 2026-06-04

### Added

- AGA 64-bit bitplane wide fetch (FMODE) + fix display corruption

### Fixed

- DENISEID $FFF8 → $00F8 for AGA Lisa

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- A1200 Stage U: AGA palette + BPLCON3 routing — and what's left
- A1200 Stage T: wire AGA registers to the chipset bus
- cargo fmt --all across the workspace
- Open Emu198x for public release
- A1200 Stage A: AGA chipset + Gayle + machine scaffold
