# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Explicitly retain the current four-bit MC68040 CACR compatibility mask
  instead of inheriting the newly modelled MC68030 register layout
- Explicitly disable the inherited MC68020/MC68030 SIZ/DSACK sequencer because
  the MC68040 uses a different external transfer protocol
- Strengthen the generated-corpus harness to preserve odd memory addresses
  and apply and compare MSP, VBR, CACR and CAAR

### Fixed

- Inherit the shared logical unaligned-data capability while retaining odd
  instruction-prefetch address errors. The MC68040 transfer protocol and odd
  MMIO split semantics remain deferred
- Inherit VBR-relative group-0 handler fetches. The current compatibility
  frame remains Format `$A`; the MC68040 Format `$7` path is deferred
- Inherit the shared MSP/ISP selection, paired master-mode interrupt frames
  and Format `$1` return compatibility behaviour
- Inherit acknowledged-vector Format/Vector consistency from the shared
  compatibility exception-entry path

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68040-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- Musashi corpora scaled 10x → 1000x; three real bugs caught
- Cpu68030 + Cpu68040 wrappers — variant pattern across the family
- 68030 + 68040 Phase-0 baselines
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
