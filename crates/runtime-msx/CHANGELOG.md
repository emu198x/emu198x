# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Six real MSX1 machines beside the two regional presets: Sony HB-10P, Toshiba HX-10, Mitsubishi ML-F80, Yamaha CX5M, Mitsubishi ML-FX1 and Sony HB-10. Each carries its slot layout, region and year from openMSX's machine configuration, and looks for its own BIOS dump before `msx.rom`.
- MSX2: two regional presets on C-BIOS's layout (`microsoft-msx2-ntsc`, `microsoft-msx2-pal`) and the Philips VG-8235, Sony HB-F1 and Panasonic FS-A1. An MSX2 takes a second firmware image, the 16 KB SUB-ROM (`msx2-sub-rom`, `msx2ext.rom`), and the main ROM falls back to `msx2.rom`. New queries: `vdp.screen_mode`, `mapper.registers` and `rtc`; `vdp` names its chip. The snapshot version is now 9.
- SCC, Sound Cartridge and FM-PAC cartridges are heard: the frame's audio mixes them with the PSG. An FM-PAC is detected by its signature on the media path. The snapshot version is now 10.

### Changed

- The MSX2 models now declare their own machine id, `microsoft-msx2`; the MSX1 models keep `microsoft-msx1`. Model ids, firmware and snapshots are unchanged.
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
