# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Six real MSX1 machines beside the two regional presets: Sony HB-10P, Toshiba HX-10, Mitsubishi ML-F80, Yamaha CX5M, Mitsubishi ML-FX1 and Sony HB-10. Each carries its slot layout, region and year from openMSX's machine configuration, and looks for its own BIOS dump before `msx.rom`.

### Changed

- Advance live-machine snapshots to version 8: the machine now carries its slot layout and subslot registers.
- Advance live-machine snapshots to version 3 and rehydrate the Z80 walker sequence before resumed execution; version 2 cannot preserve accepted interrupt response identity

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-msx-v0.2.0) - 2026-06-04

### Added

- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(msx)* operational parity — runtime crate, MCP server, shell-backed script

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(debug)* make the debug-target macros storage-agnostic
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- rustfmt the workspace clean
- workspace clippy autofix sweep
