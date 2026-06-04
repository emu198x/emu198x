# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
