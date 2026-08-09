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
   (DMC DMA stealing cycles during sprite DMA). Expected values now recorded;
   see [below](#stage-2-what-the-oracle-showed). Not yet fixed.
3. **`cpu_timing_test6`** — one ROM, one settled value to chase.
4. **`blargg_nes_cpu_test5`** — hardest. `#FF` means "some sub-test failed"
   without saying which, so the ROM's text output has to be decoded first. A
   prior investigation exists at
   `docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`.
5. **Triage the 15 visual-only ROMs.** They can never pass or fail, so under a
   gated sweep they need an explicit declared exclusion rather than a third
   category nobody revisits.

## Stage 2: what the oracle showed

The ROM reports its failure as sixteen measured clock counts and a CRC over
them. Sixteen numbers with nothing to compare them against cannot distinguish
"one cycle out" from "the wrong shape", so the first move was to obtain the
expected sixteen rather than edit DMA timing until a CRC matched. Mesen2's core
was built headless and driven through its C API by
[`tools/mesen-nes-cross-check`](../../tools/mesen-nes-cross-check/); the full
table is at
[`test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv`](../../test-data/nintendo/nes/blargg-survey/sprdma-dmc-dma-expected.tsv).

**The defect is narrow.** Every difference is `+1`, never `-1` and never more
than 1, at exactly the alignments where the reference takes the shorter path.
Emu198x is one cycle too slow at half the alignments — an alignment cycle taken
unconditionally where hardware takes it only when the get/put phase demands it.
The reference alternates by parity; Emu198x returns a flat value.

⚠ **Four candidate causes were measured and disproved**, each recorded so it is
not re-attempted:

| Hypothesis | Measurement | Verdict |
|---|---|---|
| OAM DMA length wrong | 513/514 by alignment | correct |
| DMC DMA length wrong | 3/4 by alignment | correct |
| Combined arbitration flat | alternates 515/516 as `$4014` slides | correct |
| Get/put phase inverted vs Mesen | ROM never settles at all | worse |

The third is the sharpest. Emu198x's arbitration *does* alternate correctly when
the experiment is reproduced in-process, yet returns a flat table under the real
ROM. The diagnostic probes that establish this are
`probe_oamdma_length_by_alignment`, `probe_dmc_dma_length_by_alignment` and
`probe_combined_dma_by_dmc_offset` in the `machine-nintendo-nes` lib tests.

### The bus-op diff, and what it rules out

Reproducing the ROM's experiment in-process kept producing approximations of
it, so the next step compared the two emulators directly instead. Both sides
now emit the same thing: the address of every DMA read cycle, split into
episodes at the halt. Mesen2's side is a Lua script
([`dma-trace.lua`](../../tools/mesen-nes-cross-check/dma-trace.lua)) using its
scripting API, so no struct layout has to be matched; Emu198x's side is
[`Nes::start_dma_trace`], with every read in `dma_cycle` routed through one
helper so a trace cannot silently miss a cycle. The 256th `$2004` write ends an
episode on both sides, which is what makes the two outputs line up.

⚠ **All sixteen OAM transfers — one per alignment, the ROM does no others —
match cycle for cycle, address for address.** Byte-identical, 285 lines each.
Since the read sequences are identical and every transfer has exactly 256 write
cycles, the transfers are also the same *length*.

**So the extra cycle is not inside the OAM DMA.** The defect is not the
sprite-DMA / DMC-DMA arbitration this ROM is named for, and that arbitration is
no longer listed as out of scope in the crate's own header, where it had sat
unmeasured.

What remains, and where stage 2 resumes: the intervals *between* transfers.
Logging the CPU cycle at each episode shows Mesen's first five inter-episode
intervals alternating (177041, 149765, 177905, 146541, 181383) where Emu198x's
are uniformly low (147405, 149391, 148269, 150687, 148755) — the same
flat-versus-alternating signature as the ROM's own table, at frame scale rather
than transfer scale. The untraced remainder is the DMC-only DMA episodes, which
both traces discard because neither is opened by a `$4014` write. That is the
next place to look, and it is a different subsystem from the one this stage
started on.

A separate, real gap was found and fixed on the way: a `$4015` write clearing
DMC enable did not cancel a queued transfer, so it still stole a cycle. Mesen2
splits cancel-before-halt from abort-after-halt and Emu198x now does too. ⚠ It
is **reference-matched but locally unproven** — these two ROMs never disable the
DMC mid-transfer, so no test in the corpus exercises it, and the gated sweep is
unchanged at 135/5/15 with it in place.

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
