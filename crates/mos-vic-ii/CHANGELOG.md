# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/mos-vic-ii-v0.2.0) - 2026-06-04

### Fixed

- return written value when reading VIC sprite-position registers

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Open Emu198x for public release
- C64 Seam 1: VIC-II BA/RDY audit landed
- C64 + NES Seam 4: catalogue oracle integrity
- Cov-5c wave 2: directed-test passes across five chip crates
- Run rustfmt across the workspace
- VIC-II sprite fetch spread across designated p-access cycles
- VIC-II independent border flip-flops (open-border trick)
- VIC-II unused-bit read mask; Agnus NTSC short/long line constants
- Add C64 snapshots and headless runner
- Add MOS VIC-II and wire it into C64
