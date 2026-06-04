# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
