# NES test oracle priority

**Status:** Accepted 2026-06-01. For Nintendo NES verification work, **silicon-validated NES oracles outrank CPU-generic oracles when they disagree**. blargg's NES test ROMs (`instr_test-v3/v5`, `nes_instr_test`, `blargg_apu_*`, `blargg_ppu_tests_*`, `cpu_*`, `ppu_*`, `apu_*`, etc.), Mesen2's cycle traces, and real-hardware FCEUX captures take precedence over Tom Harte's per-instruction 6502 vectors and Lorenz's C64-focused 6502 corpus when adjudicating 2A03 behaviour. Parallel to [`spectrum-test-oracle-priority.md`](spectrum-test-oracle-priority.md); same principle, NES-specific oracles.

## Context

The NES CPU is the Ricoh **2A03** — a 6502 with BCD-mode disabled, the APU and DMC sharing the die. The test stack today:

1. **Tom Harte 6502 single-step tests** — JSON corpus, ~10,000 randomised vectors per opcode. Generated from a 6502 model. CPU-generic; doesn't know about 2A03's BCD-disabled behaviour or the NES's DMA bus protocol.
2. **Wolfgang Lorenz 6502 (CPU subset)** — C64-context corpus, useful for CPU coverage but the system-dependent tests are skipped for the NES core.
3. **Dormann 6502 functional** — flat-64K functional test. Passes; pure CPU regression coverage.
4. **nestest** — system-level smoke test. 8991/8991 passing; primarily an integration sanity check.
5. **blargg's NES test ROMs** — ~155 ROMs across CPU, APU, PPU, DMA, mapper, interrupt, and timing subsystems. Run on a real NES (NTSC + PAL) by Shay Green and contemporaries. CRC-based for opcode tests; `$6000` / `$00F8` / `$00F0` / nametable for pass/fail. NES-native.
6. **Mesen2 cycle traces** — modern reference emulator, extensively cross-validated against silicon by Sour. Cycle-trace-oriented; NES-integration.
7. **Real-hardware FM2 / FCEUX captures** — deterministic input-log replay against real NES hardware. Tooling pending.

The previous adjudication wisdom from [`concepts/test-methodology.md`](../concepts/test-methodology.md) made Tom Harte the primary CPU oracle for CPU-only work. The trigger for re-stating: blargg's `instr_test-v3/v5/02-immediate`, `nes_instr_test/03-immediate`, and `instr_test-v5/02-immediate` failures all point at one disagreement — the LXA / ATX `$AB` magic constant. Our core uses `(A | 0xEE) & data` to satisfy Tom Harte; blargg's CRC expects Mesen's stable `A = operand; X = A` model. The two oracles can't both be right.

`blargg_nes_cpu_test5/official.nes` 01-implied is a similar shape: Tom Harte passes every implied opcode (ROL A, transfers, INX/INY/DEX/DEY, NOP, flag pairs); blargg's per-test CRC catches something Tom Harte's per-cycle bus probe misses. Two oracles disagree.

## Why the priority changes for NES

Tom Harte's vectors are produced from a 6502 model. The exact silicon revision being modelled is not the **Ricoh 2A03** that shipped in 1985 NES units — that chip has BCD-mode physically removed, slightly different undocumented-opcode behaviour, and a DMA pin protocol the corpus doesn't observe. For *CPU-generic 6502* work, Tom Harte is the right oracle.

For *NES* work the oracle that matters is the chip in the NES. blargg's test ROMs ran on real NES hardware and their CRCs encode silicon behaviour for that specific chip. When blargg and Tom Harte disagree on 6502 behaviour observable in a NES context, the disagreement is at most "different 6502 revisions behave differently" and at minimum "the model differs from the 2A03 silicon". Either way, for an emulator users will run real NES software on, matching blargg's result is more useful than matching Tom Harte.

The same logic applies to Mesen2. Mesen2's per-cycle trace model is cross-validated by Sour against extensive reverse-engineering of 2A03 silicon. When Mesen2 and Tom Harte disagree on a 2A03 behaviour observable in a NES context, Mesen2 wins.

This is **not** a generic statement that real-hardware-measurement always beats spec-modelling. It is a project-scoped statement: the NES is the target system, the 2A03 in the NES is the chip to model, and NES-validated oracles are the closest available proxy for that chip.

## New adjudication order

For any 2A03 behaviour observable in a NES context, the order is:

1. **Real-hardware FM2 / FCEUX replay** (when the harness lands) — definitive when reproducing a known-good real-hardware capture.
2. **blargg's NES test ROMs** — definitive for instruction-level behaviour including undocumented opcode constants, flag effects, and timing as observed on a real NES.
3. **Mesen2 cycle traces** — definitive for cycle-trace, DMA, and NES-integration behaviour (OAMDMA/DMC interleaving, interrupt timing, PPU dot-by-dot timing).
4. **nestest** — useful integration smoke. Lowest opcode-level priority but full-system regression catch.
5. **Tom Harte 6502** — useful CPU-generic regression catch. When it disagrees with the three above, follow them and accept Tom Harte regressions (with an explicit per-opcode allowlist in the test harness, same shape as the Spectrum Z80 `ACCEPTED_TOM_HARTE_DISAGREEMENTS` introduced 2026-05-31).
6. **Wolfgang Lorenz 6502, Dormann** — smoke / coverage tests. Lowest priority.

The principle: **closest to the actual 2A03 silicon wins**.

## Implications for current state

- **LXA / ATX `$AB`.** Our core uses `(A | 0xEE) & data` to pass Tom Harte's `ab.json`. blargg's `instr_test-v3/02-immediate`, `instr_test-v5/02-immediate`, and `nes_instr_test/03-immediate` all fail on the resulting CRC. **Decision: switch to Mesen's stable model `A = operand; X = A`**, accept the Tom Harte regression via per-opcode allowlist on opcode `$AB`. Three blargg fails become passes; Tom Harte stays at 100% via allowlist.

- **`blargg_nes_cpu_test5/official.nes` 01-implied CRC.** Tom Harte passes every implied opcode. blargg's CRC catches something not visible in Tom Harte's per-cycle bus probe. **Decision: investigate under the new priority** — identify the specific opcode and behaviour the CRC catches, fix to match blargg, accept any Tom Harte regression. Not in this commit; tracked here so the next session lands a focused fix.

- **Future undocumented-opcode disagreements** (ARR, ANE, SHA, SHX, SHY, TAS) follow the same rule. Tom Harte and blargg can both be silent on these or disagree; when they disagree, blargg wins.

- The headline metric on [`tests/nes.md`](../tests/nes.md) of "Tom Harte 100%" is correct as a number but misleading as a framing. Future updates should lead with blargg pass-rate and mention Tom Harte secondarily.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Adding a CPU "regression" to the Tom Harte allowlist for the NES *without* first checking whether the change matches a silicon-validated NES oracle. If yes, add to allowlist; if no, fix the bug.
- Justifying a 2A03 implementation choice with "Tom Harte passes" when a NES-validated oracle (blargg / Mesen2 / FM2) disagrees.
- Writing "but Tom Harte agreement is the gold standard" when the context is NES work — that was the pre-blargg-priority framing and this decision supersedes it.
- Considering a 2A03 silicon-revision tunable as a way to satisfy both oracles. 2A03 issue-level emulation is out of scope; the chip in a stock NES front-loader is the chip we model.
- Applying this priority to non-NES 6502 work. Other 6502 systems (C64, Apple II, BBC, Atari 8-bit) will get their own adjudication when they need it.

## What this is not

- **Not** a deprecation of Tom Harte. The 2.56M-test suite stays in CI, stays at 100% baseline (or 100% with documented per-opcode allowlist), and remains the primary catch for CPU-generic regressions. The change is what happens *when oracles disagree*, not whether Tom Harte continues to run.
- **Not** an instruction to fix every NES disagreement immediately. Some need silicon-or-Mesen evidence; until that lands, the disagreement stays documented. This decision is the framing change that makes a disagreement a *bug* rather than a permanent accepted disagreement.
- **Not** specific to LXA. The same priority applies to any future disagreement between NES-validated and CPU-generic oracles on 2A03 behaviour observable in a NES context.

## See also

- [`concepts/test-methodology.md`](../concepts/test-methodology.md) — original adjudication paragraph, now superseded for the NES case.
- [`tests/nes.md`](../tests/nes.md) — current pass rates and the sweep status.
- [`spectrum-test-oracle-priority.md`](spectrum-test-oracle-priority.md) — parallel decision for ZX Spectrum (Z80) work, written 2026-05-18.
