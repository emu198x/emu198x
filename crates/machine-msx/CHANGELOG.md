# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `MsxLayout`, `SlotAddress` and `RamRegion`, and `Msx::with_layout`: the BIOS, RAM and cartridge ports can sit in any slot or subslot, as they do across real MSX1 machines. An expanded slot has its subslot register at `$FFFF`, read back complemented. `Msx::new` keeps the old layout.
- `Msx::msx2`: the MSX2, with a V9938 on ports `$98-$9B`, the SUB-ROM in the slot `MsxLayout::sub_rom` names, a memory mapper (`MsxLayout::mapper`) on ports `$FC-$FF` and an RP5C01 clock on `$B4`/`$B5`. A BIOS image longer than 32 KB shows the rest at `$8000`, for C-BIOS's logo ROM.
- Sound beyond the PSG: the Konami SCC in `MapperType::KonamiScc` cartridges (opened by `$3F` at `$9000`, registers at `$9800`), Konami's Sound Cartridge (`MapperType::SoundCartridge`: an SCC-I, its SCC+ mode and 128 KB of RAM), the FM-PAC (`MapperType::FmPac`: a YM2413 on `$7C`/`$7D` and at `$7FF4`/`$7FF5`, banked ROM and 8 KB of SRAM), and built-in MSX-MUSIC (`Msx::install_msx_music`). `Msx::end_audio_frame` mixes them with the PSG. `detect_mapper` recognises the FM-PAC's `PAC2OPLL` signature. The SCC MegaROM now mirrors pages 2 and 1 into pages 0 and 3.

### Changed

- `Msx::vdp` is replaced by `Msx::tms9918` and `Msx::v9938`, each `None` on the other generation, and `Msx::vdp_scanline`.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-msx-v0.2.0) - 2026-06-04

### Added

- shared DebugTarget MCP debug tools + MSX/VIC-20 pilot
- *(msx)* operational parity — runtime crate, MCP server, shell-backed script
- *(borders)* TMS9918 — canonical TV-visible frame with border
- *(emu198x-msx)* MSX1 headless runner + gated BIOS-boot smoke
- *(machine-msx)* fresh-write MSX1 machine wiring + slot system + keyboard

### Fixed

- *(z80)* reliable single-instruction stepping via a retirement counter

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
