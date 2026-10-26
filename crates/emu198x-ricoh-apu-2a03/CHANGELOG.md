# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.6.1

### Fixed

- A triangle silenced by its length or linear counter holds its current
  sequencer step instead of outputting 0, so it leaves a DC level as the real
  DAC does (#1567).
- The output stream runs at exactly 48,000 Hz. The downsampler now carries
  its remainder; it emitted `cpu_hz / ceil(cpu_hz / 48,000)` before, about
  1.9% sharp on NTSC (#1568). Snapshots from 0.6.0 still restore.

### Changed

- `AUDIO_ROUTING_VERSION` is 3. Audio captured with 0.6.0 (version 1) does
  not match these fixes' output, so recorded hashes need re-capturing.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/ricoh-apu-2a03-v0.2.0) - 2026-06-04

### Added

- *(nes)* flesh out MCP server with 11 NES-specific tools
- *(nes,apu)* Apu::soft_reset + raise sweep tick cap to 150M

### Fixed

- *(nes-apu)* snapshot halt/counter at HF detect to honour silicon timing window

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Seam 3 (NES + C64): serde-skip audit lock + real PAL APU fidelity fix
- C64 + NES Seam 4: catalogue oracle integrity
- Cov-5c wave 2: directed-test passes across five chip crates
- Expand NES mapper and runtime coverage
- add native apu channel controls
- Add fresh NES headless runtime path
