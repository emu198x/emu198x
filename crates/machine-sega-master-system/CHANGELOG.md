# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose cartridge SRAM import/export and dirty tracking for battery-save sidecars

### Fixed

- Route Sega-mapper cartridge SRAM reads and writes through the two banked 16 KB windows instead of returning `$FF`

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-sega-master-system-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(sms)* operational parity — runtime crate, MCP server, shell-backed script
- *(borders)* sega-vdp — canonical TV-visible frame with border
- *(emu198x-sega-master-system)* headless runner + Alex Kidd live boot
- *(machine-sega-master-system)* fresh-write SMS / Game Gear wiring

### Fixed

- *(z80)* reliable single-instruction stepping via a retirement counter

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- clear remaining workspace clippy issues
- workspace clippy autofix sweep
