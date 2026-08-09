# Decision: NES accuracy closure campaign

**Date:** 2026-08-09
**Status:** ACTIVE
**Assessment revision:** `b7463525`

## The question

What accuracy work must Emu198x complete before the NES effort pivots from broad
improvement to failure-driven maintenance?

## Current assessment

The NES core is in better shape than its instrumentation suggested. Every case
wired to a specific harness passes — `sprite_hit` 01–11 and `sprite_overflow`
1–5 in full, which are the discriminators emulators habitually fail — and a
whole-corpus sweep of 155 ROMs reports **135 pass, 5 fail, 15 visual-only, 0
timeouts**.

The gap was never accuracy. It was that **nothing asserted the result**.

Two defects hid it, both fixed in this campaign's first stage:

1. `diagnostic_nes_suite` panicked on a missing `EMU198X_NES_SUITE`. It lives in
   the lib target, which cargo runs first, so the panic fast-failed the whole
   package and the NES ignored suite was unrunnable on any machine that had not
   set that variable. Fixed at `9db24f8d`.
2. `nes_sweep` ran the corpus, printed a verdict per ROM, tallied them — and
   passed unconditionally. The suite was green whether every ROM passed or every
   ROM failed, so its own findings were discarded at the last line.

⚠ **This is the Spectrum campaign's finding in a different form.** There the
declared gates were binary and all passing, so they could not distinguish "the
ULA is correct" from "the ULA is wrong where no gate probes". Here the
measurement existed and was thrown away. Both leave a system unable to generate
its own next question.

No numerical family score is assigned, for the reason recorded in the
[C64](c64-accuracy-closure-campaign.md), [Amiga](amiga-accuracy-closure-campaign.md)
and [Spectrum](spectrum-accuracy-closure-campaign.md) campaigns: these ROMs
assert at different boundaries — `$6000` result codes, `$00F0`/`$00F8` settle
bytes, nametable text, and visual-only output with no programmatic channel.
Combining them into one percentage would imply a weighting the evidence does not
provide.

## The goal

**Every ROM in the staged corpus is accounted for: it passes, or it carries a
recorded verdict with a stated reason.** Closure is reached when the sweep
yields no unexamined result and no undeclared change.

Not "all green". Two of the current five failures may prove to be genuine
hardware behaviour the test asserts more strictly than the silicon does; that is
a finding to record, not a defect to force. The criterion is that nothing is
unexamined.

## Evidence — sweep at `b7463525`

| | |
|---|---|
| swept | 155 |
| pass | 135 |
| fail | **5** |
| visual-only (no result protocol) | 15 |
| timeout / panic | 0 |

The five failures are three distinct defects:

| suite | ROMs | reported |
|---|---|---|
| `blargg_nes_cpu_test5` | `cpu.nes`, `official.nes` | `#FF` — a sub-test failed inside a multi-test build |
| `cpu_timing_test6` | `cpu_timing_test.nes` | `#98` — settled at `$00F0 = 0x98` |
| `sprdma_and_dmc_dma` | both variants | `#01` — "T+ Clocks", sprite-DMA / DMC-DMA contention |

⚠ Note where they are **not**. The APU suites pass, the instruction tests pass,
the MMC3 tests pass. The residual is CPU timing and DMA contention — the hardest
part, and three discrete problems rather than a broad weakness.

## Stages

Each stage is one commit's worth of work with a definite done-condition.

1. ✅ **Gate the sweep.** Record the 155 verdicts as a declared baseline and
   assert an exact match, so a regression fails *and* an unannounced improvement
   fails. Done in this stage alongside this record.
2. **`sprdma_and_dmc_dma`** — two ROMs, one defect, well-documented behaviour
   (DMC DMA stealing cycles during sprite DMA). Most tractable.
3. **`cpu_timing_test6`** — one ROM, one settled value to chase.
4. **`blargg_nes_cpu_test5`** — hardest. `#FF` means "some sub-test failed"
   without saying which, so the ROM's text output has to be decoded first. A
   prior investigation exists at
   `docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`.
5. **Triage the 15 visual-only ROMs.** They can never pass or fail, so under a
   gated sweep they need an explicit declared exclusion rather than a third
   category nobody revisits.

## ⚠ On acquiring more test ROMs

The corpus is 263 ROMs and the sweep reaches 155. Before adding suites, close
the gap between those two numbers — an unswept ROM already on disk is worth
more than a newly fetched one, and costs nothing to obtain. Candidates for later
acquisition once the current corpus is exhausted: the NESdev `ppu_sprite_hit`
and `ppu_sprite_overflow` originals, Tom Harte's 6502 per-instruction corpus
(already used for Z80 in this workspace), and `nes_cpu_exec_space`.

## Related

- [`nes-blargg-survey.md`](../processes/nes-blargg-survey.md) — how the
  measurement is taken, and the manifest pinning the ROMs it takes it from.
