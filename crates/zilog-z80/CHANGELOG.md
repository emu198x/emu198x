# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/zilog-z80-v0.2.0) - 2026-06-04

### Added

- 68000 disassembler — full ISA + effective-address strictness

### Fixed

- *(z80)* don't rename H/L to IXH/IXL when the instruction uses (IX+d)
- *(z80)* reliable single-instruction stepping via a retirement counter
- *(z80)* preserve WZ across INIR/INDR/OTIR/OTDR repeat path
- Z80 HALT leaves PC at HALT+1 (Tom Harte 100%)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(z80)* collapse per-machine stepping into a shared Z80Stepper trait
- *(z80)* correct 5 stale HALT-PC expectations to HALT+1
- point CPU test-corpus resolvers at assets/test-suites
- cargo fmt + clippy clean across the workspace
- step / run_until_pc / disasm — second half of Z80 debug suite
- query_cpu — read every Z80 register in one MCP call
- Open Emu198x for public release
- Tree housekeeping: project relocation paths + Cargo.lock
- Apply cargo fmt to three files that had drifted
- Make HALT block until IRQ instead of falling through
- +3 disk Loader now runs end-to-end (architecturally)
- Rehydrate Z80 walker sequence on snapshot restore
- Gate ZEXDOC + ZEXALL in CI; strip weird home-relative fixture paths
- Apply cargo fmt across in-tree edits + refresh Cargo.lock
- Move zilog-z80 FUSE runner to tests/ + light directed-test polish
- Split sharp-lr35902 opcode dispatcher per instruction class
- Tighten Z80 branch contention and FUSE tracing
- Add Z80 FUSE compatibility harness
- Add cached ZEX resume and verify full suites
- Add ZEX checkpoint-targeted reruns
- Wire Z80 local verification corpora into tests
- Cover more Z80 ED refresh and repeat paths
- Expand ED-prefixed Z80 integration coverage
- Add more Z80 integration coverage
- Expand Z80 execute integration coverage
- Port pin-level Z80 core into workspace
