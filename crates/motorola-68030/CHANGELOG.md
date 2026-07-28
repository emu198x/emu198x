# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Model the external CDIS input as a combinational cache override that
  suppresses hits and fills without flushing retained entries

### Changed

- Install the MC68030 CACR layout, including persistent control bits and
  always-zero instruction/data clear commands
- Inherit the MC68020 SIZ/DSACK data-transfer sequencer and reinstall its
  capability after deserialization
- Strengthen the generated-corpus harness to preserve odd memory addresses
  and apply and compare MSP, VBR, CACR and CAAR

### Fixed

- Preserve inherited warm instruction-cache contents across serialization
- Correct the MC68EC030 capability description: it removes the on-die MMU but
  retains the external MC68881/MC68882 coprocessor interface
- Inherit MC68020 logical unaligned-data transactions while retaining odd
  instruction-prefetch address errors. Odd MMIO split semantics remain
  deferred where responder lanes are unresolved
- Inherit VBR-relative group-0 handler fetches and the rejected
  next-instruction address in the Format `$A` PC field
- Inherit MC68020-family MSP/ISP selection, paired master-mode interrupt
  frames and Format `$1` return behaviour
- Inherit acknowledged-vector Format/Vector consistency from the shared
  MC68010 exception-entry path

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68030-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- Open Emu198x for public release
- A1200 Stage B: Cpu68020 swapped into the A1200 machine
- Cpu68030 + Cpu68040 wrappers — variant pattern across the family
- 68030 + 68040 Phase-0 baselines
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
