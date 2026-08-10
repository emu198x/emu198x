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
2. ✅ **`sprdma_and_dmc_dma`** — two ROMs, one defect, well-documented behaviour
   (DMC DMA stealing cycles during sprite DMA). Fixed at `1aa4eb67`: the DMC
   gained the `$4015` transfer-start delay it was missing. All 32 alignment
   values now match the Mesen2 oracle and both ROMs report Passed. See
   [below](#stage-2-what-the-oracle-showed) for the evidence and
   [the outcome](#stage-2-outcome-the-two-path-model-was-right).
3. **`cpu_timing_test6`** — one ROM, one settled value to chase.
4. **`blargg_nes_cpu_test5`** — hardest. `#FF` means "some sub-test failed"
   without saying which, so the ROM's text output has to be decoded first. A
   prior investigation exists at
   `docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`.
5. **Triage the 15 visual-only ROMs.** ⚠ Two things are already known about
   the `dmc_tests` four, measured rather than assumed: Mesen2 confirms they
   carry no `$6000` protocol at all, and their nametable RAM holds no ASCII, so
   the text reader that works for the `$6000`-era suites returns nothing. They
   draw tile indices against a CHR font. Gating them needs tile-index decoding
   or framebuffer comparison, not the existing text path -- which matters
   because `dmc_tests/latency.nes` is the natural gate for the DMC
   transfer-start delay and is not cheaply available. See
   `probe_dmc_tests_text`. Until then they can neither pass nor fail, so a
   gated sweep needs an explicit declared exclusion for them rather than a
   third category nobody revisits.

   ⚠ **The Spectrum ROM-font decoder does not transfer**, checked when stage 2
   closed. `common-sinclair-zx-spectrum::screen_text` (added at `da0f91e7`)
   decodes a *known* font out of ROM against display-file geometry. The
   `dmc_tests` ROMs are a harder problem in kind: their iNES header declares
   **0 KB of CHR**, so the font is uploaded to CHR RAM at runtime, and their PRG
   contains no ASCII at all -- the only printable run in the whole 16 KB is a
   block of `U` filler. Recovering their text means rendering CHR RAM glyphs and
   matching them against a reference font supplied from outside the ROM, which
   is OCR against an unknown font rather than decoding a known one. Framebuffer
   comparison against Mesen2 is the cheaper route and stays the stage-5 plan.

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

### ROOT CAUSE: the DMC transfer-start delay is missing

A `$4015` write that re-arms an idle DMC does not start its DMA immediately on
hardware. Mesen2's `DeltaModulationChannel::WriteRam`:

```cpp
InitSample();
//Delay a number of cycles based on odd/even cycles
//Allows behavior to match dmc_dma_start_test
if((_console->GetCpu()->GetCycleCount() & 0x01) == 0) {
    _transferStartDelay = 2;
} else {
    _transferStartDelay = 3;
}
```

`ProcessClock` then counts it down and calls `StartDmcTransfer` on the cycle it
reaches zero. **Emu198x sets `dma_pending` immediately, with no delay.**

⚠ The delay is **2 or 3 by get/put parity**. A zero delay is parity-independent,
which is exactly why every measurement in this campaign found Emu198x flat where
the reference alternates. It accounts for the whole failure:

| Observation | Explained by |
|---|---|
| `+1` at exactly half the alignments, never `-1` | delay is 2 or 3, we use 0 |
| Emu198x's table flat where Mesen alternates | zero delay cannot vary with parity |
| re-arm→fetch latency 11 where Mesen is 12 | one cycle of the missing delay |
| Mesen sweeps 12, 11, 10, 9; Emu198x only 11, 10, 9 | the sweep loses its first case |
| 30 sample fetches against Mesen's 35 | the sweep terminates an iteration early |

Measured with `probe_dmc_rearm_vs_fetch` against
[`dmc-fetch-cycles.lua`](../../tools/mesen-nes-cross-check/dmc-fetch-cycles.lua).

**The fix** belongs in `ricoh-apu-2a03`: hold a countdown on `$4015` re-arm
instead of raising `dma_pending` at once. ⚠ The delay's length depends on the
CPU's get/put parity, which the APU does not currently know — the machine owns
it as `cpu_cycle_count`. Deciding how the APU learns the phase is the design
question the fix has to answer first, and it should not be guessed at.

⚠ Mesen's comment names `dmc_dma_start_test` as the ROM this behaviour exists
to satisfy. That suite is in the corpus and should gate the fix.

### Stage 2 outcome: the two-path model was right

Fixed at `1aa4eb67`. The root-cause reading above held, and the sequence that
proved it is worth keeping because two earlier attempts had failed here.

**The decisive step was removing behaviour, not adding it.** Making `$4015`
enable set `current_address` and `bytes_remaining` and *nothing else* — no
request, no delay — was run on its own first, as a falsifiable test of the
model. The re-arm-to-fetch latency immediately walked 14, 13, 12, 11, 10, 9,
against a baseline that managed only 11, 10, 9 before the write pre-empted the
walk and truncated it. Writes at a 433-cycle cadence, fetches at 432: the
vernier the model predicted.

At that point all 16 alignments already tracked Mesen2's relative shape exactly
and were uniformly 3 cycles short — an alignment-independent constant, which is
what a missing fixed-length delay looks like. Adding `transfer_start_delay`
(2 or 3 by parity) supplied it. All 32 rows then matched the oracle and both
ROMs reported Passed.

Why the two prior attempts failed: both made `transfer_start_delay` the fetch
*trigger*, which deletes the timer-driven path along with the defect. The delay
is an **additional** path for the cold-start case, not a replacement for
buffer-consumption requests. Ordering the work as remove-then-measure-then-add
is what separated the two effects; had the delay gone in first, its uniform +3
would have been invisible underneath the still-flat table.

Two further points the fix settled:

* **A second, independent defect** sat in `clock_output`: the request came after
  the `if`/`else`, so it also fired on the silence path where the buffer was
  already empty and hardware requests nothing. It now sits inside the
  buffer-consumed branch, guarded by `transfer_start_delay == 0` so a
  `$4015`-armed start owns the next fetch.
* **The open question is answered.** A traced `transferStartDelay` that stayed 0
  at an idle-channel enable was a polling artefact, not a hidden guard: the
  delay is 2–3 cycles and `ProcessClock` decrements it every CPU cycle, so a
  Lua poll can miss the whole transient.

The parity is published by the machine as `Apu::cpu_cycle_odd`. It must be the
CPU cycle counter the DMA arbiter aligns on — the APU's own `odd_cycle` counts
APU cycles and is free to be out of phase.

⚠ `dmc_dma_start_test` is **not** in the local corpus, so it did not gate this.
The two `sprdma_and_dmc_dma` ROMs did, against the 32-row Mesen2 oracle.

### Superseded reading: it is the DMC channel, not the DMA

⚠ **Correction to the reading below.** The inter-transfer intervals turned out to
be an *effect*, not the cause: they differ by exactly 1.00 NTSC frames (29 636
against 29 780.7 CPU cycles/frame) at T+00, T+02 and T+04, which is a one-cycle
error crossing a vblank sync window and costing a whole frame. A symptom
amplifier, not a location.

Following it down localised the defect. Mesen2's DMC sample address is a
constant `$E3C0`, so a Lua read callback on that one address logs the CPU cycle
of every sample fetch — no operation type needed, which Lua callbacks do not
receive. Comparing fetch cycles against Emu198x's DMA episode trace:

| Mesen2 | Emu198x | |
|---|---|---|
| 1228471 | 1228472 | +1 |
| 1228903 | 1228904 | +1 |
| 1229335 | 1229336 | +1 |
| 1229767 | 1229768 | +1 |
| 1230199 | *(none)* | Mesen fetches once more here; Emu198x does not |

Emu198x's fetches run exactly one cycle late, then Mesen fits in an extra fetch
that Emu198x never performs, after which the two sequences desynchronise. Over
the same window Mesen makes **35 sample fetches to Emu198x's 30**, and Emu198x's
DMC falls silent for ~129 000 cycles where Mesen keeps fetching every 3 424.

⚠ **The DMA length is not the difference.** Emu198x's DMC-only DMAs are 4 cycles
in 25 of 27 cases, which looked like the flat-versus-alternating signature until
Mesen's fetch spacing was measured at the same 432 cycles (428 period + 4 DMA).
Both take 4 there. The hypothesis is dead; what differs is **how often the
channel asks at all** — its timer and sample-restart scheduling, in
`ricoh-apu-2a03`, not the arbitration in `machine-nintendo-nes`.

⚠ The two runs' boot alignment differs by ~119 cycles at the first fetch, so
absolute cycle comparisons need care. The gap *sequences* are the trustworthy
signal, and they diverge structurally: Mesen `402, 432, 432, 432, 432, 3050,
4378, 2470, 3424…` against Emu198x `284, 432, 432, 432, 3050, 518, 3858, 2472,
3424…`.

### Superseded reading: the intervals between transfers
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
