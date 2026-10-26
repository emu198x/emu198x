# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## 0.6.1

### Added

- A CMOS core, `M6502::new_65c12` (`Variant::Cmos`): the 65SC02 instruction
  set of the BBC Master's 65C12. It adds `BRA`, `STZ`, `TSB`, `TRB`, `PHX`,
  `PHY`, `PLX`, `PLY`, `INC A`, `DEC A`, the new `BIT` modes, `(zp)` and
  `JMP (abs,X)`; runs the old illegal opcodes as defined `NOP`s; fixes the
  `JMP (abs)` page bug; spends a cycle on decimal `ADC` and `SBC` and takes N
  and Z from the decimal result; reads rather than writes in a
  read-modify-write's middle cycle; and clears D on every interrupt. It
  passes Tom Harte's Synertek 65C02 suite cycle for cycle, apart from the
  bit instructions, `WAI` and `STP`, which the 65C12 does not have, and the
  NMOS core still passes the NMOS suite in full.
- `M6502::variant` and `M6502::set_variant`. The variant is not serialised,
  so existing snapshots keep their layout; a machine with a CMOS part sets
  it again after restoring one.

### Fixed

- Keep the NMI edge detector running while RDY holds the CPU, so an edge
  that arrives during a hold is still taken, and block new edges during an
  interrupt's status push and first vector read, as the real detector does.
  A short pulse there is lost; a level still asserted afterwards is taken
  when the detector reopens. The Atari 8-bit's ANTIC NMIs depend on both.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/mos-6502-v0.2.0) - 2026-06-04

### Added

- *(nes-cpu)* adopt Mesen LXA model + add NES test-oracle priority decision

### Fixed

- *(6502+nes)* cold-boot SP = \$00, sweep cooldowns post-reset
- *(6502)* NMI hijacks BRK / IRQ vector fetch
- *(6502)* suppress IRQ poll on taken non-page-cross branch
- NES NMI edge detection on an instruction's final cycle
- 6502 decimal-mode ADC N/V flags (Tom Harte 100%)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(nes-cpu)* foundation Rust port of blargg 01-implied CRC framework
- *(lorenz)* emulate 6510 zero-page port pull-up/down/float
- *(c64)* Lorenz sweep — correct overly-conservative skip list
- *(c64)* Lorenz sweep — port nes_sweep pattern
- *(6502)* clarify LXA magic-constant trade-off
- point CPU test-corpus resolvers at assets/test-suites
- Open Emu198x for public release
- remove spurious NMI boundary edge-detect (blargg vbl_nmi 04)
- Close Cov-2 + Cov-3: tick.rs correctness paths + 68000 carve-out
- Close Cov-1: hermetic decode-table sweep for mos-6502
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- 6502 penultimate-cycle IRQ/NMI sampling + CLI/SEI/PLP one-instr delay
- Document 6502 penultimate-cycle interrupt sampling gap
- CIA 6526 SP rate + 50/60Hz selector; 6502 BCD Oxyron flag semantics
- Update test fixtures for 7-cycle 6502 reset + expose reset_phase
- 6502 RDY stall + proper 7-cycle reset
- Tighten 1541 IEC behavior and trace helpers
- Finish 6502 external verification coverage
- Add 6502 verification harnesses and fix indexed reads
- Add C64 snapshots and headless runner
- Add MOS 6502 core and C64 CPU loop
