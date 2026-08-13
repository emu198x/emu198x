# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Direct CPU-cadence regression proving that a `NOP` consumes four machine T-states

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-jupiter-ace-v0.2.0) - 2026-06-04

### Added

- Z80 group on the shared debug tools (9 machines + Sord M5)
- *(borders)* jupiter ace — TV-visible frame with white border
- extract Jupiter Ace from donor codebase

### Fixed

- Jupiter Ace boots to its cursor
- *(z80)* reliable single-instruction stepping via a retirement counter

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- rustfmt the workspace clean
- clear remaining workspace clippy issues
- tail of the clippy autofix sweep
- *(jupiter-ace)* document display as deliberately inline (not a chip)
