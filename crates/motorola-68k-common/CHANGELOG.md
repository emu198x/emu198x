# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add MC68020/MC68030 SIZ and DSACK representations, phase sizing,
  read-lane selection and write-data duplication helpers
- Add capability-gated MC68020-family A7 selection across USP, ISP and MSP
  without interpreting the reserved M bit on MC68000 or MC68010 register files
- Add shared interrupt-acknowledge address helpers for carrying the
  accepted level on A3-A1 through the current MC68000-shaped shared
  core and its inherited test harnesses

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68k-common-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
