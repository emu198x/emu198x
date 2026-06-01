# Spectrum test oracle priority

**Status:** Accepted 2026-05-18. For ZX Spectrum verification work, **Spectrum-validated oracles outrank CPU-generic oracles when they disagree**. Patrik Rak's `z80test` (run on real 48K Spectrum hardware), FUSE's cycle traces, and real-hardware RZX recordings take precedence over Tom Harte's per-instruction vectors and Frank Cringle's ZEXALL when adjudicating Z80 behaviour. Supersedes the adjudication paragraph in [`concepts/test-methodology.md`](../concepts/test-methodology.md#reference-adjudication) for the Spectrum case.

## Context

The Z80 test stack now has five oracles:

1. **Tom Harte** — per-instruction JSON state vectors. Generated from a Z80 model. CPU-generic.
2. **ZEXDOC / ZEXALL** — Frank Cringle's CRC-based exercisers. CP/M `.COM` files run on a flat 64K. CPU-generic.
3. **FUSE** — 1,356 single-instruction cycle traces (event sequence + register state + T-state count). From the FUSE Spectrum emulator. Cycle-trace-oriented and Spectrum-context.
4. **Patrik Rak `z80test`** — seven TAP exercisers (`z80full`, `z80doc`, `z80flags`, `z80docflags`, `z80ccf`, `z80memptr`, `z80ccfscr`). Each computes CRCs over the documented + undocumented effects of every instruction and compares against values measured on a real 48K Sinclair ZX Spectrum. Spectrum-native.
5. **Real-hardware RZX replay** (harness pending). Captures a deterministic input log against real Spectrum hardware; replay must reproduce identically. Spectrum-native, integration-grade.

[`concepts/test-methodology.md`](../concepts/test-methodology.md#reference-adjudication) made Tom Harte the primary CPU oracle and FUSE a "strong secondary reference" — written before `z80test` was in scope. With `z80test` landed (2026-05-18) the priority needs re-stating.

The trigger is concrete. Wiring up `z80test` surfaced two `z80memptr` failures (`102 INIR->NOP'`, `103 INDR->NOP'`) — direct siblings of the two FUSE accepted disagreements (`edb2_1 INIR`, `edba_1 INDR`). The pattern across all four cases is identical:

- **Tom Harte** agrees with our current Z80.
- **FUSE** disagrees — expects `WZ` to end non-zero.
- **Patrik Rak `z80test`** disagrees — same shape.

Two independent Spectrum-validated oracles disagree with us. One CPU-generic oracle agrees with us. The previous policy would record the disagreements and leave them unresolved indefinitely. That is no longer acceptable for the Spectrum SOLID work.

## Why the priority changes for Spectrum

Tom Harte's vectors are produced from a Z80 model. The exact silicon revision being modelled is not the chip that shipped in 1982 Sinclair 48K boards, and the model may codify behaviour from a different revision or from a simplified specification rather than measured silicon. For *CPU-generic* work — testing whether a Z80 core matches "a Z80" — this is the right oracle.

For *Spectrum* work the oracle that matters is the chip in the Spectrum. Patrik Rak's TAPs were validated against an actual Sinclair board running an actual Zilog Z80. When Patrik Rak and Tom Harte disagree on Z80 behaviour, the disagreement is at most "different Z80 revisions behave differently" and at minimum "the model is wrong about the silicon". Either way, for an emulator users will run real Spectrum software on, matching the Patrik Rak result is more useful than matching Tom Harte.

The same logic applies to FUSE. FUSE's cycle traces are measured against the FUSE team's Spectrum hardware reference. When FUSE and Tom Harte disagree on a Z80 behaviour observable in a Spectrum context, FUSE wins.

This is **not** a generic statement that real-hardware-measurement always beats spec-modelling. It is a project-scoped statement: the Spectrum is the target system, the chip in the Spectrum is the chip to model, and Spectrum-validated oracles are the closest available proxy for that chip.

## New adjudication order

For any Z80 behaviour observable in a Spectrum context, the order is:

1. **Real-hardware RZX replay** (when the harness lands) — definitive when reproducing a known-good real-hardware capture.
2. **Patrik Rak `z80test`** — definitive for instruction-level behaviour including MEMPTR, X/Y flags, CCF/SCF chains.
3. **FUSE** — definitive for cycle-trace and Spectrum-integration behaviour (memory contention, I/O contention, interrupt timing).
4. **Tom Harte** — useful CPU-generic regression catch. When it disagrees with the three above, follow the three above and accept Tom Harte regressions.
5. **ZEXDOC / ZEXALL** — useful smoke tests. Lowest priority.

The principle: **closest to the actual Spectrum silicon wins**.

## Implications for current state

- The two `z80memptr` failures (INIR/INDR MEMPTR) and the two FUSE INIR/INDR accepted disagreements are now tracked as **bugs to fix**, not accepted disagreements. They share a root cause and a fix.
- The fix needs silicon-level evidence to land confidently. Open research item filed in [`Emu198x-Reference/_organised/known-unknowns.md`](../../../../Emu198x-Reference/_organised/known-unknowns.md): investigate INIR/INDR MEMPTR behaviour on real Zilog Z80 silicon (Ken Shirriff Z80 decap work, z80.info pages, Visual6502 if Z80 coverage exists, contemporary Sinclair-era reports).
- Once the silicon-level question resolves, the Z80 core's INIR/INDR MEMPTR handling gets fixed, the `z80memptr` allowlist in `crates/machine-sinclair-zx-spectrum-48k/tests/z80test.rs` is removed, the FUSE accepted-disagreement count drops from 6 to 4, and Tom Harte's pass count likely drops by ≤ 2 (the matching instruction tests will start failing). All three changes ship together in one commit, with a wiki update explaining the trade.
- The headline metric on [`tests/spectrum.md`](../tests/spectrum.md) of "Tom Harte 100%" is correct as a number but misleading as a framing. Future updates should lead with Spectrum-validated pass rates and mention Tom Harte secondarily.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Adding more entries to the FUSE accepted-disagreement table or the `z80memptr` allowlist *without* first checking whether the new disagreement points the same way (Spectrum-oracle says A, our core says B, Tom Harte agrees with us). If yes, it's a bug, not a disagreement.
- Justifying a Z80 implementation choice with "Tom Harte passes" when a Spectrum-validated oracle (FUSE / Patrik Rak / real-hardware RZX) disagrees.
- Writing "but Tom Harte agreement is the gold standard" when the context is Spectrum work — that was the pre-`z80test` framing and this decision supersedes it.
- Considering a Z80 silicon-revision tunable (e.g. "Issue 2 vs Issue 3 Z80 behaviour") as a way to satisfy both oracles. Z80 issue-level emulation is out of scope. The chip in a stock 48K is the chip we model; that's all the variance we need.
- Applying this priority to non-Spectrum work. Other systems will get their own adjudication: NES uses Mesen + nestest + Blargg, C64 uses VICE, Amiga uses WinUAE / Moira. Each system's oracle priority belongs in that system's own decision record.

## What this is not

- **Not** a deprecation of Tom Harte. The 1.6M-test suite stays in CI, stays at 100% as the baseline, and remains the primary catch for CPU-generic regressions. The change is what happens *when oracles disagree*, not whether Tom Harte continues to run.
- **Not** an instruction to fix the INIR/INDR MEMPTR bug immediately. The fix needs silicon evidence; until that lands, the allowlist remains and the bug stays tracked. This decision is the framing change that makes the bug a bug rather than a permanent accepted disagreement.
- **Not** specific to MEMPTR. The same priority applies to any future disagreement between Spectrum-validated and CPU-generic oracles on Z80 behaviour observable in a Spectrum context.

## Update 2026-05-31 — the trigger MEMPTR cases resolved

The decision did its job. The two `z80memptr` failures (`102 INIR->NOP'`,
`103 INDR->NOP'`) and the matching FUSE `edba_1 INDR` case are all now
**passing** — turned out not to need silicon-level evidence at all.

Root cause: the INIR/INDR/OTIR/OTDR repeat handler in `execute.rs` was
writing `WZ = PC + 1` after the IN/OUT step had already correctly set
`WZ = BC ± 1` per the documented MEMPTR behaviour. The stale write was
a leftover "we'll re-execute" marker from the pre-2026 vectors. Removing
that single line satisfied both Spectrum-validated oracles in one change.

Tom Harte's pass count did not drop. Instead the suite's per-opcode
allowlist now carries four documented WZ-only entries for
`ed b2 / b3 / ba / bb`, reflecting that Tom Harte's pre-2026 vectors
record the old marker. This is exactly the trade the decision predicted
in § Implications, paragraph 3 — modulo Tom Harte being kept green via
allowlist rather than dropping pass count.

The four residual FUSE block-I/O disagreements are now AF-only (X/Y
undocumented flag bits on the final repeat iteration); WZ is correct
across all four. Those remain tracked as
[`Emu198x-Reference/_organised/known-unknowns.md`](../../../../Emu198x-Reference/_organised/known-unknowns.md)
§ Zilog Z80, and are correctness debt, not launch-blockers.

## See also

- [`concepts/test-methodology.md`](../concepts/test-methodology.md) — original adjudication paragraph, now superseded for the Spectrum case.
- [`tests/spectrum.md`](../tests/spectrum.md) — current pass rates and the FUSE / `z80test` disagreement tables.
- [`../../../Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md`](../../../../Emu198x-Reference/_organised/by-topic/testing-suites/spectrum-test-roms.md) — Reference catalogue of Spectrum test ROMs.
