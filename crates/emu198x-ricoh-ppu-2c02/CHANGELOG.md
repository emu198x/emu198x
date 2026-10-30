# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.1.1

### Added

- Capture and restore per-bit bus-refresh timestamps from a containing machine save.
- Restore a validated colour/emphasis output mirror from a containing machine save, preserving partial-field composite-video output without changing the PPU’s own serialized layout.

## 0.1.0

### Added

- `PpuCartridge`, the cartridge connector from the PPU's side: CHR reads
  and writes, nametable mirroring, nametable overrides, and the A12 and
  PPU-read notifications mappers count. The PPU's `tick`, `run`,
  `cpu_read` and `cpu_write` take `&mut dyn PpuCartridge` where they took
  the cartridge crate's `&mut dyn Mapper`.
- `Mirroring` is defined here rather than re-exported from the cartridge
  crate. The PPU now depends on no other Emu198x crate.
- First release under the `emu198x-ricoh-ppu-2c02` name. Until now the crate was
  `ricoh-ppu-2c02`, versioned with the Emu198x suite and unpublished; the entries below are from that name.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/ricoh-ppu-2c02-v0.2.0) - 2026-06-04

### Added

- *(nes)* document PPU overscan, expose 256 × 224 TV-visible helper
- *(ppu)* per-bit decay model for PPU open bus
- cycle-accurate PPU sprite evaluation

### Fixed

- mask unimplemented sprite-attribute bits on OAM write
- delay $2001 rendering-enable for the odd-frame dot-skip
- route OAM DMA writes through OAMADDR
- raise the PPU sprite overflow flag for a real 9th sprite
- power up NES palette RAM with the canonical table
- NES NMI edge detection on an instruction's final cycle

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(nes)* Phase 4 — CPU cycle phase split + PPU NMI realignment
- *(nes)* Phase 1 — Ppu::run(target) + ppu_clock field
- Open Emu198x for public release
- C64 + NES Seam 4: catalogue oracle integrity
- Cov-5c wave 2: directed-test passes across five chip crates
- Improve NES MMC5 audio and IRQ accuracy
- Expand NES mapper and runtime coverage
- add MMC3 mapper support
- Add fresh NES headless runtime path
