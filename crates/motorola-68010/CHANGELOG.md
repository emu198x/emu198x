# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68010-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- A1200 Stage M: BF on (An)/(An)+/-(An) — +6.8K unique PCs to FC1xxx
- cargo fmt --all across the workspace
- Open Emu198x for public release
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- Cpu68030 + Cpu68040 wrappers — variant pattern across the family
- M68k test-oracle strategy + inherited-subset cross-validation
- 68020 Phase 7.6: variant-gate BCD V + DIV overflow
- 68020 Phase 7: continuation hook + RTD
- 68020 Phase 6: 6-word exception frame + M-flag
- 68020 Phase 1.5: bring the 68010 crate to life
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
