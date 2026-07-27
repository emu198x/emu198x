# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Inherit acknowledged-vector Format/Vector consistency from the shared
  MC68010 exception-entry path

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68020-v0.2.0) - 2026-06-04

### Fixed

- AGA Workbench palette (68020 full-format EA decode)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage M-2..M-5: BF on all memory EA modes (extension-word path)
- A1200 Stage M: BF on (An)/(An)+/-(An) — +6.8K unique PCs to FC1xxx
- cargo fmt --all across the workspace
- Open Emu198x for public release
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- Musashi corpora scaled 10x → 1000x; three real bugs caught
- Cpu68030 + Cpu68040 wrappers — variant pattern across the family
- M68k test-oracle strategy + inherited-subset cross-validation
- 68020 Phase 6 closeout: Format \$2 frames for CHK / divide-by-zero / TRAPV / Trace
- 68020 Phase 6: 6-word exception frame + M-flag
- 68020 Phase 5f: bit-field family
- 68020 Phase 5a: 32-bit MULL / DIVL
- 68020 Phase 3: scaled-index brief extension word
- 68020 Phase 1.5: bring the 68010 crate to life
- 68020 Phase 1: fork Cpu68020 from the type alias
- 68020 Phase 0: Tom Harte harness + baseline measurement
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
