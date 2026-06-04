# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-nintendo-nes-v0.2.0) - 2026-06-04

### Added

- *(nes)* document PPU overscan, expose 256 × 224 TV-visible helper
- *(nes)* implement CPU bus open bus + unmap \$4020-\$5FFF
- *(nes,apu)* Apu::soft_reset + raise sweep tick cap to 150M
- *(nes)* soft reset + sweep $81-protocol handling
- grade blargg's on-screen-result PPU tests

### Fixed

- *(6502+nes)* cold-boot SP = \$00, sweep cooldowns post-reset
- mask unimplemented sprite-attribute bits on OAM write
- route OAM DMA writes through OAMADDR
- repair the nestest full-run harness setup

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(nes)* reclassify test_ppu_read_buffer.nes as VISUAL after walking its boot
- *(nes)* probe harness for blargg_nes_cpu_test5 advancing CRC
- *(nes)* classify volumes.nes as visual
- *(nes)* bump per-ROM tick budget to 200M
- *(nes)* raise settle threshold to 10M ticks
- *(nes)* widen sweep to 11 more blargg suites (155 ROMs total)
- *(nes)* classify silent-on-pass DMC tests as visual
- docs(nes) + test(nes): official.nes investigation + sweep classifier
- *(nes)* document the MAX_TICKS dead-end
- *(nes)* nametable text grader (Passed / Failed)
- *(nes)* clippy clean (collapse if-let, use contains)
- *(nes)* classify visual demos as Verdict::Visual
- *(nes)* multi-protocol sweep grader ($F8 / $F0 settle)
- *(nes)* Phase 4 — CPU cycle phase split + PPU NMI realignment
- *(nes)* Phase 3 — machine drives PPU via run(target)
- *(nes)* Phase 2 — internal_master_clock at 4× resolution
- NES test-rom sweep across untouched directories
- cargo fmt the nestest fixture_dir closure
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Seam 3 (NES + C64): serde-skip audit lock + real PAL APU fidelity fix
- Seam 2 (NES + C64): host input → controller / joystick routing
- NES blargg: drop sprite_hit + sprite_overflow tests from harness
- NES Seam 1: blargg PPU harness landed (baseline 6/28 passing)
- Improve NES MMC5 audio and IRQ accuracy
- Expand NES mapper and runtime coverage
- add native apu channel controls
- Update test fixtures for 7-cycle 6502 reset + expose reset_phase
- Add fresh NES headless runtime path
