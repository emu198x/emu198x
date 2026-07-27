# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Expose the internally rejected odd-address transfer as a diagnostic
  `AddressErrorObservation`, without claiming an external bus transfer
- Add a binary `SingleStepTests/m68000` harness covering a 240,090-row
  agreement subset and a separate 55,606-event address-error taxonomy

### Changed

- Gate instruction-cache use and allocation on the MC68030 external
  cache-disable input without invalidating retained entries
- Clear CACR and invalidate the installed variant instruction cache on reset
- Preserve one serialized MC68020/MC68030 logical data transfer across
  responder-sized physical phases while retaining the existing byte/word
  compatibility response for unchanged machines and test harnesses
- Pin the `SingleStepTests/680x0` full sweep to its exact fixture count,
  two named invalid rows, classified software-oracle differences and a
  row-stable compatibility fingerprint
- Resolve the registered 680x0 corpus from the shared 198x assets root by default

### Fixed

- Gate odd-address data rejection by CPU capability so MC68020-family
  wrappers retain odd start addresses for logical data transactions while
  MC68000 behaviour remains unchanged. Dynamic sizing applies where a machine
  reports responder width; unresolved odd MMIO semantics remain machine work
- Complete the documented 16-word extent of the MC68020 Format `$A` short
  bus-fault frame and preserve its entry/RTE continuation through serde.
  Precise special-status, pipeline, data-buffer and fault-rerun state remains
  incomplete
- Use VBR for inherited MC68010-and-later group-0 handler fetches while
  retaining the MC68000's fixed vector base
- Stack the rejected next-instruction address as the MC68020 Format `$A` PC
  when an odd instruction-boundary prefetch takes vector 3
- Preserve the supervisor stack bank, saved SR and saved PC throughout an
  in-flight formatted `RTE`, including an MC68020 Format `$1` restart
- Support the inherited MC68020 master-interrupt transition from an ordinary
  MSP frame to a matching Format `$1` throwaway frame on ISP
- Build MC68010-and-later interrupt Format/Vector words from the vector
  selected by acknowledge, including device-supplied and spurious vectors
- Serialize the pending MC68010-and-later exception-frame PC instead of
  restoring an in-flight formatted frame with a zero saved address
- Latch lower-to-level-7 transitions until an interrupt boundary instead of
  treating a continuously held level 7 as a new request at every instruction
- Select spurious vector 24 when external BERR terminates an
  interrupt-acknowledge cycle, while retaining ordinary vector-2 bus errors
- Carry the accepted interrupt level on A3-A1 during acknowledge and
  update the active SR mask when interrupt processing begins instead of
  re-sampling or deferring those effects until vector completion
- Preserve group-0/group-1 processing state through handler prefetch so
  recursive faults and the address-error I/N bit follow the exception
  sequence
- Use program-space function codes for PC-relative operand reads and
  supervisor-data function codes for exception-vector reads

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/motorola-68000-v0.2.0) - 2026-06-04

### Added

- 68000 disassembler — full ISA + effective-address strictness

### Fixed

- *(68000)* correct group-8 SBCD/OR decode overlap
- *(68000)* correct group-C ABCD/EXG/AND decode overlap
- *(68000)* decode DBcc by its real size field, not Scc/ADDQ
- AGA Workbench palette (68020 full-format EA decode)

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- *(68000)* cross-check the decoder against the Asm198x isa-disasm spec
- cargo fmt + clippy clean across the workspace
- A1200 Stage N: CPU interrupts_taken counter — IRQs ARE firing (89K/10s)
- A1200 Stage M-2..M-5: BF on all memory EA modes (extension-word path)
- A1200 Stage M: BF on (An)/(An)+/-(An) — +6.8K unique PCs to FC1xxx
- A1200 Stage L: 68010+ interrupt frames push F/V word; +17K unique PCs
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Musashi corpora scaled 10x → 1000x; three real bugs caught
- 68020 Phase 7.6: variant-gate BCD V + DIV overflow
- 68020 Phase 6 closeout: Format \$2 frames for CHK / divide-by-zero / TRAPV / Trace
- 68020 Phase 7.5: Musashi-style BCD V flag
- 68020 Phase 7: continuation hook + RTD
- 68020 Phase 6.5: 16-bit DIV overflow C preservation
- 68020 Phase 6: four-word formatted exception frame + M-flag write support
- 68020 Phase 3: scaled-index brief extension word
- 68020 Phase 1.5: bring the 68010 crate to life
- Fix more clippy lints from Rust 1.95.0
- Post-track tidy: rustfmt sweep + motorola-68000 doc accuracy
- Reduce motorola-68000 to truly-M68000
- Split 68k family into per-variant crates + strip MMU/FPU from M68000
- Add Amiga postcard snapshots across the chip stack
- SID noise taps + ADSR rates + TEST; CIA 6526 alarm; 68000 cycle fixes
- Tighten 68000 Harte coverage and fixture handling
- Add fresh Amiga headless baseline
