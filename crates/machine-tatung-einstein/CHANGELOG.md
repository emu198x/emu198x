# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Remove the fictional NTSC machine configuration; the Einstein TC-01 is a
  PAL-only TMS9129A system.
- End `run_frame` at the TMS9918A raster wrap instead of its earlier VBlank
  interrupt, eliminating the short power-on frame.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-tatung-einstein-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(einstein)* operational parity — runtime crate, MCP server, shell-backed script
- *(tatung-einstein)* port machine + binary + gated VDP-init smoke

### Fixed

- *(z80)* reliable single-instruction stepping via a retirement counter

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
