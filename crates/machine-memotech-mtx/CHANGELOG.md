# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- End `run_frame` at the TMS9918A raster wrap instead of its earlier VBlank
  interrupt, eliminating the short power-on frame.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-memotech-mtx-v0.2.0) - 2026-06-04

### Added

- *(mtx)* route the VDP interrupt through the Z80 CTC
- *(mtx)* boot to BASIC Ready — support paged ROMs (OS + BASIC + ASSEM)
- Z80 group on the shared debug tools (9 machines + Sord M5)
- extract Memotech MTX from donor codebase

### Fixed

- *(z80)* reliable single-instruction stepping via a retirement counter
- *(mtx)* correct port-$00 paging and I/O map so boot completes hardware init

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- rustfmt the workspace clean
