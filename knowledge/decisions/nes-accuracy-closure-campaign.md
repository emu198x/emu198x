# Decision: NES accuracy closure campaign

**Date:** 2026-08-09
**Status:** ACTIVE
**Assessment revision:** `b7463525`; stages 1-4 closed 2026-08-10

## The question

What accuracy work must Emu198x complete before the NES effort pivots from broad
improvement to failure-driven maintenance?

## Current assessment

The NES core is in better shape than its instrumentation suggested. Every case
wired to a specific harness passes — `sprite_hit` 01–11 and `sprite_overflow`
1–5 in full, which are the discriminators emulators habitually fail — and a
whole-corpus sweep of 155 ROMs reported **135 pass, 5 fail, 15 visual-only, 0
timeouts** at the campaign's start.

The gap was never accuracy. It was that **nothing asserted the result**.

⚠ **That reading proved truer than intended.** Of the five opening failures,
**one was a real defect and four were grading defects** — the harness reporting
failures the emulator never had. The sweep now stands at **140 pass, 0 fail, 15
visual-only**, reached with a single change to an emulator crate (the DMC
transfer-start delay, stage 2). Stages 3 and 4 changed only the grader.

⚠⚠ **Zero fails is not proof of correctness.** 15 ROMs are visual-only and
ungraded, and 99 `.nes` files on disk sit in directories the sweep never
enumerates. The campaign's goal was never "all green" — see below — and the
honest statement is that no *graded* ROM currently fails.

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

⚠ **In the event, four of the five were not hardware behaviour and not defects
either — they were the harness misreading its own inputs.** The distinction the
goal anticipated (defect vs stricter-than-silicon) missed a third category:
verdicts manufactured by the grader. That category cost more of this campaign
than the one genuine defect did.

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
3. ✅ **`cpu_timing_test6`** — ⚠ **not an emulator defect.** The ROM was
   passing all along; the harness misgraded it. Closed at stage 3 with no
   change to any emulator crate. See
   [below](#stage-3-the-defect-was-in-the-grader).
4. ✅ **`blargg_nes_cpu_test5`** — ⚠ **not an emulator defect either.** Both
   ROMs pass; the `#FF` was a misread sentinel. Closed with no change to any
   emulator crate. See [below](#stage-4-the-sentinel-that-was-not-one). This
   supersedes `docs/handoffs/2026-05-30-nes-official-cpu-test5-investigation.md`,
   whose central inference was wrong.
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

## Stage 3: the defect was in the grader

`cpu_timing_test.nes` passes, and has been passing throughout this campaign. It
prints `PASSED` on screen at 54.6M ticks. The `#98` the sweep reported was never
a result code.

**What `$00F0 = 0x98` actually is.** The ROM's shell (`console.a`) uses `$F0`/`$F1`
as a pointer while uploading its font to CHR RAM at init — `$98` is the low byte
of `chr_data`. Nothing touches `$F0` again for the remaining 16 seconds, so it
sits perfectly steady and the settle heuristic read it as a verdict.

**Two independent grader defects had to line up.** Either alone would have been
caught:

1. The `$F0`/`$F8` settle channel treated *any* steady non-zero byte as a result
   code, on the protocol's "1 = pass, other = fail with that code" reading. That
   is an **inference** — "this stopped changing" — dressed as a declaration.
2. The nametable channel only knew the mixed-case `$6000`-era vocabulary
   (`Passed`/`Failed`). This ROM's older shell prints `PASSED` in upper case, so
   the one channel that could have spoken did not match.

The settle channel fires at 10M ticks; the ROM prints its verdict at 54.6M. The
weak channel therefore always won the race, 44M ticks before the strong one had
anything to say.

**Measured before fixing.** Instrumenting the sweep to report which channel
decided each verdict showed the settle channels deciding 43 of 155 ROMs — and
**42 of those settle at exactly `0x01`**, the protocol's defined pass code. The
non-`1` branch had fired exactly once in the entire corpus, on this ROM, and was
wrong. That is what justified demoting it rather than tuning it.

**The fix, in the grader only.** A settled value of `1` still decides a pass
immediately. Any other settled value is now held as a fallback and the ROM keeps
running, so a positive channel gets its chance; the fallback is reported only if
nothing else speaks by the tick ceiling. The nametable channel learned the older
shell's vocabulary (`PASSED`, `FAIL OP`, `UNKNOWN ERROR`, `BASIC TIMING WRONG`).

⚠ **This is the campaign's own theme a third time.** Stage 1 found a sweep that
computed verdicts and discarded them. Here the measurement was taken, kept, and
*attributed to the wrong source*. A harness that can fabricate a failure is worse
than one that stays silent: it sends real work after a defect that does not
exist. This one absorbed part of the campaign's stated fault budget from the
outset.

⚠ **Coverage note, not a defect.** The ROM tests **official instructions only**
by default; its readme documents holding B for official + all undocumented, or A
for official + `$EB` + unofficial NOPs. The sweep boots it with no buttons held,
so undocumented-opcode *timing* remains untested. Worth a controller-holding
variant, and cheap now that the ROM grades correctly.

## Stage 4: the sentinel that was not one

`cpu.nes` and `official.nes` both pass. All eleven sub-tests pass in both. No
emulator crate changed.

**What `$00FF = 0xFF` is.** Not a result code. The 2026-05-30 handoff inferred
it was one — "`$00FF == 0xFF` after a run is a reliable indicator of a
`blargg_nes_cpu_test5`-family fail" — and the sweep's grader was built on that
sentence. **Mesen2 ends with `$00FF == 0xFF` too.** It is residue, exactly like
`cpu_timing_test`'s `$00F0`.

**What the missing marker is.** The shell marks each *passing* sub-test with a
`$00` tile at column 31 — one row BELOW that sub-test's name. So the last
marker lands on the separator line and `01-implied`'s own row is always bare.
Counted correctly there are **eleven markers for eleven sub-tests**. The handoff
read the bare first row as "01-implied failed" and spent its analysis on which
implied opcode was at fault. None was.

**Three independent measurements agree**, which is what makes this safe to
declare rather than merely plausible:

| Evidence | Result |
|---|---|
| All 20 of `01-implied`'s expected CRCs, taken from `source/01-implied.a`, searched for in zero page during the run | all 20 observed, in order, ~171 000 ticks apart |
| Emu198x's nametable vs Mesen2's, byte for byte, both ROMs | identical, markers included |
| `$00FF` after the run, both emulators | `0xFF` in both |

The CRC check is the strongest of the three because it is independent of both
the screen and the reference: blargg published the correct checksums in the
source, and our CPU produces every one of them.

⚠ The ROM never printed a failing opcode. `instr_test_end.a`'s `@wrong` handler
prints the opcode and mnemonic on any CRC mismatch, and sampling the screen
throughout the run showed it never fired. That alone should have been read as
"nothing failed" rather than "the detail scrolled away".

**The grader now counts markers against sub-test rows** instead of reading
`$00FF`. Tooling added: `nametable-dump.lua` and `zp-sentinel.lua` in the
cross-check harness, and `probe_implied_checksums` /
`probe_cpu_test5_raw_nametable`.

## ✅ Coverage gap closed (2026-08-10)

The sweep reached 155 of the 263 `.nes` files on disk. **Every directory is now
either swept or named in `UNSWEPT_DIRS` with a reason**, and
`every_directory_is_accounted_for` asserts that — a new directory fails the
suite until someone decides about it. That test is deliberately NOT `#[ignore]`d:
it only reads directory names, and the gap it closes survived because noticing
it required going to look.

The sweep is now **171: 151 pass, 2 fail, 0 timeout, 18 visual**.

⚠ The 99 un-swept files were mostly not tests. What they actually were:

| Category | Directories |
|---|---|
| Real test suites, now swept (+16 ROMs) | `mmc3_irq_tests`, `mmc3_test_2`, `read_joy3` |
| Already gated elsewhere | `mmc3_test` (in `blargg_ppu.rs`) |
| Demos, games, homebrew | `other` (39), `blargg_litewall`, `240pee`, `nes15`, `ny2011`, `scanline`, `scanline-a1`, `scrolltest`, `spritecans-2011`, `stomper`, `tutor`, `window5`, `nrom368` |
| Need peripherals or a human | `PaddleTest3`, `vaus-test`, `tvpassfail`, `MMC1_A12`, `m22chrbankingtest` |
| Wrong region | `pal_apu_tests` |
| Visual, no result protocol (MMC5) | `mmc5test`, `mmc5test_v2`, `exram` |

**The two new failures are expected and are not defects.** `mmc3_irq_tests`'s
own readme: *"The last two ROMs test different revisions of the MMC3, so at most
only one will pass on a particular emulator."* We implement MMC3B, so `rev_B`
passes and `rev_A` fails; `mmc3_test_2`'s `5-MMC3` / `6-MMC3_alt` are the same
pair. It is the same reason `blargg_ppu.rs` leaves `mmc3_test`'s `6-MMC6`
unwired. The real result here is **ten new MMC3 IRQ-counter passes**, covering
scanline timing and A12 clocking, which nothing previously gated.

### ⚠ Two capability gaps this surfaced

1. ~~No PAL machine.~~ ✅ **Closed — see below.** All ten `pal_apu_tests` pass,
   and seven of them discriminate by region.
2. ~~Three MMC5 ROMs render nothing.~~ **Resolved — see below. They were never
   blank; the harness was reading a buffer that is always empty for MMC5.**

## The MMC5 "blank screens": a harness blind spot, not a defect

The three MMC5 ROMs render correctly. Their screens match Mesen2 **byte for
byte**. The blank was in the observer.

**MMC5 keeps its nametable RAM inside the mapper** and can map ExRAM or a fill
tile into any of the four `$2000-$2FFF` slots. The PPU already routed reads and
writes through `Mapper::nametable_read`/`nametable_write`, so rendering was
always right — but every tool that inspected the screen read
`ppu.nametable_ram()`, the console's CIRAM, which for an MMC5 cartridge is
**never written at all**. The sweep's nametable grader was structurally blind to
mapper 5.

⚠ Note what nearly happened. The evidence — three ROMs, blank screen, mapper
implemented — supported "possible MMC5 defect", and that was how it was first
recorded. What refuted it was checking a channel the suspect code could not
influence: **the framebuffer**, which showed 10 and 20 distinct colours and
~16 000 non-background pixels. This is the fourth time in this campaign that a
confident reading of an emulator defect came from an instrument rather than the
emulator.

### What changed

* `Mapper::nametable_peek(&self, addr)` — a side-effect-free view, defaulting
  to `None`. It has to exist separately because `nametable_read` takes
  `&mut self`: MMC5 drives its scanline detector off nametable fetches, so a
  debugger reading the screen through it would corrupt the timing it is trying
  to observe.
* `Nes::effective_nametable()` — mapper first, CIRAM fallback. **This is the
  accessor screen-reading tools should use.** `ppu.nametable_ram()` answers a
  narrower question than it appears to.
* The sweep's nametable grader now uses it, so any future mapper that serves
  its own nametables is graded rather than silently timing out.

### The verdicts

All three are **visual** — no result protocol. `mmc5test` and `mmc5test_v2`
draw with a custom graphics font (no ASCII to match at all); `mmc5exram` is a
colour-bar demo. That is now a measured classification rather than an
assumption.

The capability worth keeping is gated on its own: `mmc5_executes_code_from_exram`
in `tests/mmc5_screen.rs`. `mmc5exram.nes` copies its per-frame bar routine into
ExRAM and runs it from `$5C00-$5FFF` during VBLANK — "A proper emulator will be
able to handle this without any problems", per the ROM's own text. The gate
asserts both the banner (through the effective nametable) **and** a
non-uniform framebuffer, because a ROM that drew its banner and then died in
ExRAM would satisfy the first check alone.

Sweep: **174 — 151 pass, 2 expected fails, 0 timeout, 21 visual.**

## PAL support

`Nes::new_with_region(mapper, Region::Pal)` exists, and all ten
`pal_apu_tests` ROMs pass — a suite that could not be graded at any clock the
emulator could previously produce.

**Why this did not violate the clock decision.**
[`nes-clock-topology.md`](nes-clock-topology.md) already anticipated it: *"the
CPU : PPU ratio stays at 1:3.2 on PAL, which is why PAL and NTSC use different
PPU tick budgets."* The master oscillator still drives the loop in both regions.
Only the dividers move.

The blocker was that 3.2 dots per CPU cycle cannot be expressed by a counter of
whole dots. The `% 3` divider became a **phase accumulator over master-clock
units** — 4 per dot and 12 per cycle on NTSC, 5 and 16 on PAL. NTSC is
arithmetically identical to the counter it replaces, which the unchanged sweep
(174 — 151/2/0/21) confirms; PAL yields the 3, 3, 3, 3, 4 pattern the ratio
demands.

| | NTSC | PAL |
|---|---|---|
| master-clock units per dot | 4 | 5 |
| units per CPU cycle | 12 | 16 |
| dots per CPU cycle | 3 | 3.2 |
| pre-render line | 261 | 311 |
| odd-frame dot skip | yes | **no** — the 2C07 has no short frame |
| APU tables | `ApuRegion::Ntsc` | `ApuRegion::Pal` |

⚠ **The gate was checked before it was trusted.** Ten green tests prove nothing
about PAL if the ROMs pass under NTSC too — the same trap as a mode-selecting
ROM whose held button never registered. `probe_pal_roms_discriminate` runs each
ROM in both regions: **seven settle at 1 on PAL and 2 or 3 on NTSC.** The three
that agree across regions (`01.len_ctr`, `02.len_table`, `03.irq_flag`) are
region-insensitive and are kept for the APU behaviour they assert, not as PAL
evidence.

**PAL PPU geometry is now gated too** — `tests/pal_geometry.rs`, six assertions
against the documented 2C07 numbers: 312 scanlines, a 106 392-dot frame, a
70-line VBLANK (241–310), and 66 495 CPU cycles per two frames, which is the
3.2 ratio's half-cycle surviving rather than being rounded away.

⚠ The dot-skip pair is the one to read carefully. `pal_never_skips_a_dot` is
worthless on its own: if the ROM never enabled rendering, NTSC would not skip
either and the test would pass proving nothing. `ntsc_does_skip_a_dot_on_odd_frames`
is its control, asserting that the same ROM on NTSC **does** produce a 340-dot
frame. Both are needed; neither means much alone.

⚠ **What this still does NOT establish.** No PAL *video output* has been compared
against a reference — the geometry gate proves the machine counts dots, lines
and cycles like a 2C07, not that what those dots contain is right. Forcing
Mesen2 into PAL needs a settings path the cross-check harness does not yet have
(the PAL test ROMs carry no PAL header flag, so its auto-detection reads them as
NTSC), which is why that comparison is not here.
`Region` is fixed at construction by design — it is read every tick, and
changing it mid-run would leave the PPU's dot counter and the CPU phase
accumulator on different clocks.

## Stage 5 attempt: two findings, and the four ROMs are not gateable by screen

**Not delivered**, and one long-standing belief in this record was wrong.

### ⚠ CORRECTION: `dmc_tests` do not draw anything

This record has said since stage 2 that the four `dmc_tests` ROMs "draw tile
indices against a CHR font", and that gating them needed tile-index decoding or
framebuffer comparison. **That is false.** They produce no screen output at all.

Measured two ways:

* **Mesen2 writes nothing to either nametable across 2400 frames** (~40 s
  emulated), with power-on RAM forced to zeros so "blank" means "never written"
  rather than "buried in noise".
* Emu198x's own bus trace over 60M ticks:

  | ROM | PPU reg writes | APU reg writes | nametable non-zero |
  |---|---|---|---|
  | `latency` | 9 (3 × `$2001`) | 81 | **0** |
  | `buffer_retained` | 9 | 39 | **0** |
  | `status` | 9 | 39 | **0** |
  | `status_irq` | 9 | 42 | **0** |

Nine PPU register writes is enabling and disabling rendering. **These ROMs
report by beeping** — the APU is their only output channel. No screen-based
gate can ever work on them, whatever is done about determinism, and
`latency.nes` cannot become the DMC gate by that route.

Gating them means comparing **audio** — the APU register write sequence, or the
DMC's internal state trace, against a reference. A different mechanism from
anything this campaign has built, and honest to call unstarted rather than
blocked.

⚠ The wrong belief survived this long because it was plausible and never
measured: "visual-only" was inferred from "no `$6000` protocol and no ASCII in
the nametable", and "draws with a custom font" was the natural explanation. The
two-run determinism check is what finally made the nametable readable enough to
show there was nothing in it.

### Second attempt: all 17 render, and the nametable comparison works

All 17 non-`dmc_tests` visual ROMs **do** produce structural screen state — but
five of them (`full_palette` ×3, `nmi_sync` ×2) write **no nametable bytes at
all**; they render entirely through palette RAM. A nametable-only survey
reported them as never drawing, which is the `dmc_tests` mistake in a new
costume: the first survey script checked one channel and would have declared
five working ROMs dead.

Mesen2 goldens for all 17 are captured and committed at
`test-data/nintendo/nes/screen-goldens/` (nametable + palette + OAM, frame 600,
reproducible).

✅ **14 of the 17 are gated** in `tests/screen_goldens.rs`. Two problems stood
between the first all-17-fail run and that, and neither was an emulator defect.

**1. The two emulators were sampling different moments.** Mesen2's `endFrame`
fires at **scanline 240, cycle 0** — measured with `where-endframe.lua`, not
assumed. Our `run_frame` returns 21 scanlines later at the wrap to scanline 0,
by which point the NMI handler has rewritten palette RAM for the next frame.
These ROMs rewrite the palette many times per frame, so "the palette at end of
frame" means nothing until both sides name the same PPU position. The gate now
runs to the Nth occurrence of (scanline 240, dot 0).

⚠ The tempting shortcut — drop the palette from the signature — would have
passed all 17 while comparing nothing on five of them, since `full_palette` ×3
and `nmi_sync` ×2 write no nametable bytes at all.

**2. Palette mirroring has to be resolved before comparing.** `$3F10`, `$3F14`,
`$3F18` and `$3F1C` mirror `$3F00/$04/$08/$0C`. Our PPU redirects writes at
`mirror_palette_addr`, so the **raw** 32-byte array keeps power-on values in
those four slots while the PPU never reads them; Mesen2's memory dump resolves
the mirror. Unresolved, that reported four differences on every ROM, none real.

### ⚠ Three withheld, and two of them look like a real defect

Not "cannot be gated" — the goldens exist and the comparison runs. Withheld
because a permanently red gate teaches people to ignore the suite.

**`dma_2007_read` and `double_2007_read` — ⚠ CORRECTION: not a defect, and not
gateable this way.**

These were briefly recorded here as a candidate defect, on the strength of a
uniform `+1` against Mesen2 across two independent ROMs. **Their own source
headers refute it:**

```
dma_2007_read.s    "33 44 or 44 55"   crc "159A7A8F or 5E3DF9C4"
double_2007_read.s "(depends on CPU-PPU synchronization)"
                   five listed outputs, four listed CRCs
```

Ours prints `44 55` — the **second documented-correct answer**. Mesen2 lands on
the first. Neither is more right; the outcome turns on CPU-PPU alignment at
reset, which the ROM states in its first ten lines.

⚠ **The lesson is about the oracle, not the emulator.** A reference emulator
captures **one draw from a set of legal behaviours**. Every use of Mesen2 in
this campaign has assumed its answer is *the* answer, which held while the ROMs
were deterministic — and silently stops holding for ROMs that admit several.
Before reading a divergence from a reference as a defect, check whether the ROM
allows more than one outcome. These two say so plainly, and it cost a wrong
"candidate defect" claim to notice.

A single golden cannot gate a multi-outcome ROM. The right gate is the ROM's
own CRC check, which accepts any legal output — a different mechanism, not
built.

### ✅ Resolved by reading the ROM's own CRC

`jsr print_crc` ends each of these ROMs, and the source header lists every
acceptable checksum. Reading the printed value settles both cases:

| ROM | prints | documented | verdict |
|---|---|---|---|
| `dma_2007_read` | `5E3DF9C4` | `159A7A8F` or `5E3DF9C4` | ✅ **correct** — gated in `tests/dmc_dma_read4_crc.rs` |
| `double_2007_read` | `D84F6815` | `85CFD627`, `F018C287`, `440EF923`, `E52F41A5` | ⚠ **DEFECT** |

**`double_2007_read` is a genuine defect** — the campaign's second, after the
DMC transfer-start delay. Not the multiple-legal-outputs situation: the ROM
enumerates its acceptable checksums and ours is outside the set.

The screen localises it. Line 1 is `22 33 44 55 66`, matching the documented
first line. Line 2 is `33 44 55 66 77`, where **every** legal variant begins
`22`, `02` or `32` — so the first byte of the *second* read is wrong and the
rest follow from it.

Per the ROM's header: *"Double read of `$2007` sometimes ignores extra read,
and puts odd things into buffer."* Two reads of `$2007` in immediate succession
(`lda $20F7,x` with `x=$10`) colliding with a DMC DMA leave our read buffer
holding a value the hardware never produces.

Its gate is written and withheld in `tests/dmc_dma_read4_crc.rs`, ready to
enable as the assertion for that fix.

⚠ **Method note, and a correction to it.** The CRC route was expected to carry
over to the four audio-only `dmc_tests` — blargg's ca65 framework checksums all
console output, so a ROM that prints nothing should still accumulate one.

**It does not carry over.** `dmc_tests` ships **no source and no readme**: four
`.nes` files and nothing else, unlike every other suite here. The CRC gate
works by comparing against the checksums the ROM's *own author* published; with
no source there is no published set, and these ROMs print nothing on screen to
read one from. The PRG contains no ASCII at all beyond a block of `U` filler.

So the four remain ungateable by any mechanism this campaign has built. What
would work is comparing the APU register write sequence, or a DMC state trace,
against Mesen2 — but note what that changes: **correctness would be defined by
Mesen2 rather than by blargg.** Every gate built here so far ultimately rests
on a value the test's author published; that one would not. Worth doing, worth
labelling honestly when it is.

⚠ **Superseded 2026-08-10 — and the Mesen-defined trace was never necessary.**
The four report their code *audibly*, and blargg published the encoding. See
[the section below](#the-dmc_tests-beep-their-verdict).

**`test_ppu_read_buffer`** diverges wholesale on the palette rather than by an
offset, and reports through custom CHR tiles plus audio. Different in kind;
needs its own investigation.

⚠ **Resolved 2026-08-10, and both halves of that sentence were wrong.** See
[the section below](#test_ppu_read_buffer-two-stacked-mistakes).

### The baseline needed a third category

The first instinct was to leave these 14 as `visual`, on the grounds that the
sweep still cannot grade them. That was wrong, and worth recording as an error
rather than quietly fixing.

`visual` in this baseline means **nobody checks this ROM**. Once a ROM has a
real gate that statement is false, and a stale "unexamined" label is exactly
the condition this campaign spent its whole length undoing. But `pass` would
claim the sweep graded it, which it did not.

So the sweep gained a `gated` verdict carrying *where* the gate lives:

```
Total: 174  Pass: 151  Fail: 2  Timeout: 0  Gated: 14  Visual: 7
```

Seven ROMs remain genuinely ungraded, and each has a reason: the four
`dmc_tests` are audio-only and never draw, and `dma_2007_read`,
`double_2007_read` and `test_ppu_read_buffer` have gates written but withheld
pending the divergence above.

⚠ Superseded: `test_ppu_read_buffer` is graded by the sweep from 2026-08-10,
so the baseline is now `Pass: 152 … Visual: 6`.

### The determinism blocker, now solved

The plan was sound and the reasoning still holds: these ROMs cannot be read as
text (no `$6000`, no result byte, no ASCII, font uploaded to CHR RAM), but they
can be **compared**. Tile indices, palette entries and sprite state are what the
PPU was *told* to draw, and unlike rendered pixels they are emulator-independent.
Mesen2 runs them correctly, so its structural screen state is the oracle.
`tools/mesen-nes-cross-check/screen-state.lua` captures exactly that and works.

✅ **Solved.** Mesen2's NES default is `RamState::Random`: Nametable RAM,
palette RAM and OAM all power up randomised. Two consecutive Mesen runs of
`dmc_tests/latency.nes` differ on **every line** of the dump — all 30 nametable
rows, the palette and OAM — because the bytes the ROM writes are buried in
power-on noise. A golden captured this way would freeze one RNG draw and assert
nothing.

That two-run comparison is the technique worth keeping: **before trusting any
reference capture, run the reference twice.** If it does not reproduce itself,
it cannot arbitrate anything. It cost one command and saved a gate that would
have failed for the wrong reason forever.

**Fixed in `main.cpp`.** `SetNesConfig` with `RamPowerOnState = AllZeros`,
called before `LoadRom`. The struct is **not** replicated — `main.cpp` now
includes Mesen2's own `Shared/SettingTypes.h` (built with `-I Core -I .`), so
the 14 184-byte layout is exact by construction and a snapshot update becomes a
compile error rather than silent corruption. Re-running the two-run check now
reports the capture reproducible.

That makes the cross-check harness usable as a golden source for **any** ROM
that does render — which is most of stage 5's remaining list. It is only these
four that are out of reach, and for a different reason.

## `test_ppu_read_buffer`: two stacked mistakes

Closed 2026-08-10 with **no emulator change**. The ROM passes, and always did.
Both things this record previously said about it were wrong, and they were
wrong in ways worth keeping.

### Mistake 1 — the palette difference was a sampling artifact

The structural gate sampled at frame 600. At that frame Mesen2's palette and
ours disagreed on all 32 bytes while the nametable's 960 bytes and OAM's 256
matched exactly. A 32-byte disagreement surrounded by 1 216 bytes of agreement
looked like a narrow, specific defect.

It was not a defect. The ROM displays a still image for 666 frames while its
longest sub-test runs — the readme says so: *"In order to distract you with
entertainment, art is provided. Contemplate on the art while the test is in
progress."* Sampling the phase boundaries on both sides
(`tools/mesen-nes-cross-check/palette-phases.lua` and
`probe_palette_phase_boundaries`) gives:

| | art phase starts | art phase ends | duration |
|---|---|---|---|
| Mesen2 | CPU cycle 17 865 828 | 37 699 641 | 666 frames |
| Emu198x | CPU cycle 18 997 602 | 38 831 415 | 666 frames |

⚠ Quoted in CPU cycles, not frames, and see the correction in
[the later section](#chasing-the-38-frame-divergence) for why. The gap is
1 131 774 cycles — **38 frames** — and it is identical at both ends.

At frame 600 Mesen had entered the art phase and we had not. **Two different
phases of the same correct sequence were being compared.** Our art-phase
palette is byte-identical to Mesen's golden, and so is the settled one. The
nametable matched throughout only because the text does not change across the
boundary — which is exactly why it gave no warning.

⚠ **The lesson generalises a rule this campaign already learned once.** Stage 5
established that a comparison needs both sides at the same PPU *position*
(scanline 240, dot 0 — not a frame counter). The same argument applies to the
*frame*: a golden is only meaningful once the screen has **settled**. The fix
is procedural, and now written into `screen-state.lua`: before capturing a
golden, check the ROM has stopped changing.

### Mistake 2 — it was never a screen-only ROM

The sweep listed it as `visual` with a long comment asserting it "reports
pass/fail via screen + audio" and that "plain ASCII scanning can't read the
verdict". Both claims were inferred from the readme's description of the
*display*, never measured against `$6000`.

It writes the standard blargg report. `$6001-$6003` hold `DE B0 61`, the text
at `$6004` ends "Passed", and `$6000` goes `$80` → `$00`. `CnRom` already
carries work RAM at `$6000-$7FFF` for exactly this reason, and its doc comment
names this ROM.

The only real obstacle was time: the ROM reports at ~520M master ticks against
a `MAX_TICKS` of 200M — about 1 450 frames where the ceiling allows ~560. It
now has a per-ROM budget in `SLOW_ROMS`, and the sweep grades it `pass` on the
author's own protocol, which is a better gate than any golden. Its frame-600
golden has been deleted rather than recaptured.

⚠ **The trap worth naming.** `MAX_TICKS` carries a note that raising the
ceiling to 250M was tried and flipped nothing. That experiment could not have
flipped this ROM: it was on `VISUAL_ROMS`, so raising the ceiling never ran it.
**A ROM excluded from the sweep is excluded from the sweep's experiments too**,
and the exclusion cites a timeout the exclusion itself made permanent.

### Left open — a 38-frame timing divergence

We reach the art phase 38 frames after Mesen, and every phase
boundary from frame 466 onward carries the same offset with identical
durations. Frames 1-58 agree exactly, so one thing between frames 58 and 466
costs us 38 frames.

`probe_nametable_change_frames` localises it to a single sub-test loop that
updates one nametable row 31 times — the sprite-0-hit / `$4014` DMA / RAM
mirroring test. **Same iteration count in both**, different period: a flat 12
frames for us, a repeating 12,10,10 for Mesen.

The period-3 cadence suggested CPU/PPU alignment that fails to rotate. That was
measured and **acquitted**: our per-frame CPU cycle count alternates
29 781/29 780, which is right for an 89 342/89 341-dot pair with the odd-frame
dot skip active.

Chased further on 2026-08-11 — see [the section below](#chasing-the-38-frame-divergence).

## PAL video: the blocker is gone, and the instrument is the wrong one

Worked 2026-08-10. The recorded blocker was that Mesen2's region
auto-detection reads the PAL test ROMs as NTSC, because none of them carries a
PAL flag in its iNES header — so a "PAL golden" was really an NTSC capture
under a PAL filename.

**That blocker is removed.** `main.cpp` honours `EMU198X_MESEN_REGION=pal`
through the same `NesConfig` struct that already forced `RamPowerOnState`, and
`region-check.lua` reads back what Mesen is *actually* running rather than what
was asked for: `region=Pal`, `clockRate=1662607`. ⚠ The read-back matters. A
setting that is silently ignored produces captures that look exactly like
successful ones.

### The finding that changes the plan

With region forcing available, the obvious next move was to capture structural
goldens under PAL and compare. Before trusting them, the same control this
campaign applies everywhere: **does the measurement discriminate?**

`pal_screen.rs::region_sensitivity_of_each_rom` runs each candidate on our own
machine under both regions and diffs the structural signature:

| ROM | NTSC vs PAL |
|---|---|
| `nmi_sync/demo_pal.nes` | identical |
| `window5/colorwin_pal.nes` | identical |
| `other/window_old_pal.nes` | identical |
| `other/window2_pal.nes` | identical |
| `nes15-1.0.0/nes15-PAL.nes` | identical |

**All five are region-blind.** Nametable, palette RAM and OAM are byte-identical
under both regions, because their region-dependence lives entirely in raster
timing. A structural PAL gate would have passed just as happily on an NTSC
machine — a test that passes when the thing it names is broken.

⚠ This is the standing lesson arriving from a new direction. Every earlier
instance was about a *test* that could not fail; this is about an *instrument*
that cannot see. Both are caught by the same question, asked before the result
is believed rather than after.

### The instrument it needs

Rendered pixels, compared in a way that does not assume a shared palette. Two
emulators need not agree on the RGB of NES colour `$21`; Mesen2 ships several
palettes. But if both drew the same picture, their framebuffers agree **up to a
bijection on colours** — so replacing each pixel with the index of its colour's
first appearance in raster order cancels the palette exactly, and the index
images must then match byte for byte. `tools/mesen-nes-cross-check/screen-pixels.lua`
implements that capture, encoded one character per pixel so 240 rows fit inside
Mesen's 500-row script log.

⚠ **Blocked**: Mesen2's headless PPU frame buffer reads back all black — one
distinct colour across all 61 440 pixels, indistinguishable from "the ROM drew
nothing". Four hypotheses eliminated:

- the `noVideo` argument to `InitializeEmu` (cleared it; no change)
- `EmulationFlags::MaximumSpeed` skipping frame rendering (disabled it; no change)
- Lua table indexing (it is 1-based, confirmed; `emu.getPixel` reads black too)
- the Lua API itself — `emu.takeScreenshot` returns a 258-byte PNG, a blank image

So it is not the capture path. Next step is inside Mesen rather than outside it:
`NesConsole::GetPpuFrame` hands out `_ppu->GetScreenBuffer(false)`; find which
of the two output buffers that selects and whether the headless build ever
fills it. Stopped there deliberately rather than trying a fifth flag.

## The `dmc_tests` beep their verdict

Closed 2026-08-10. The record said these four were ungateable except by a
Mesen2-defined trace, and that was wrong in the most useful way: the gate that
was missing rests on a value blargg published, like every other gate here.

### What the record got right, now measured

They have no `$6000` protocol. No `DE B0 61` signature, and `$6000-$6007` all
zeroes after 900M ticks — over 1.7× the budget `test_ppu_read_buffer` needed.
⚠ That claim was previously *inferred*, and the identical inference about
`test_ppu_read_buffer` had just turned out to be wrong, so it was worth the
five minutes to measure rather than inherit.

### The channel that was there all along

blargg's shell readme (`ppu_open_bus/readme.txt`) documents an audible result:

> A byte is reported as a series of tones. The code is in binary, with a low
> tone for 0 and a high tone for 1, and with leading zeroes skipped. The first
> tone is always a zero. A final code of 0 means passed.

| Tones | Binary | Code |
|---|---|---|
| low | `0` | 0 — passed |
| low high | `01` | 1 — failed |
| low high low | `010` | 2 |
| low high high | `011` | 3 |

⚠ The readme attributes the tones to NSF builds, and these four are `.nes`.
They emit them anyway — measured, not assumed in either direction.

**The gate reads the code's LENGTH, not its value**, and that is enough:
because leading zeroes are skipped and the first tone is always the zero, code
0 is one tone and every non-zero code is two or more. "Exactly one tone" is
exactly "passed". All four beep once.

### The counter is under test, by three ROMs with known codes

A tone counter that always returned 1 would pass all four gates while proving
nothing — the failure mode this campaign has met repeatedly. So the same
function is pointed at three ROMs whose codes are established through a
completely separate channel, `$6000`, read by the sweep:

| ROM | `$6000` | expected tones | measured |
|---|---|---|---|
| `mmc3_irq_tests/1.Clocking` | `$00` | 1 | 1 |
| `mmc3_irq_tests/5.MMC3_rev_A` | `$03` | 3 | 3 |
| `mmc3_test_2/6-MMC3_alt` | `$02` | 3 | 3 |

⚠ Honest limit: that shows the counter discriminates on blargg shell ROMs, not
on `dmc_tests` specifically. No failing build of these four exists to aim it at.

### Decoding the value was attempted and abandoned

Within one ROM the two tones are an octave apart — `6-MMC3_alt` beeps
222/444/222 Hz for code 2, textbook `010`. But absolute pitch is not portable:
`mmc3_irq_tests` beeps near 440 Hz where `mmc3_test_2` beeps near 222. And
autocorrelation on the APU's mix ties across harmonics — 440, 221 and 147 Hz
score identically on the same burst — so a low/high classifier picks a
sub-harmonic as often as a fundamental. Zero-crossing counting was worse: DC
offset made one low tone read anywhere from 25 Hz to 400 Hz.

Two estimators, both rejected, so the third was not attempted. The decoder
**refuses rather than guesses** when the tones do not span enough pitch, and
`probe_tone_shape` keeps the measurements for whoever wants the value as well
as the verdict.

### The corpus has no unexamined ROMs left

```
Total: 174  Pass: 152  Fail: 2  Timeout: 0  Gated: 20  Visual: 0
```

⚠ `Visual: 0` means every ROM has a **named gate**, not that everything is
verified. The gates differ in strength, and the standing warning still holds:
absence of a failing gate is not evidence of correctness where no gate runs.

## Chasing the 38-frame divergence

Worked 2026-08-11. Not closed, but narrowed from "38 frames somewhere in a
600-frame run" to a single wait, a single threshold, and one specific unanswered
question. Four candidate causes were measured and acquitted, and one wrong claim
was made and retracted along the way.

### Where the frames go

The sub-test loop spends 92% of its time in one VBlank-wait subroutine:

```text
$EBCE  BIT $8E      ; skip the wait entirely if the flag is set
$EBD0  BMI $EBDA
$EBD2  BIT $2002    ; clear the VBL flag
$EBD5  BIT $2002    ; wait for it to be set again
$EBD8  BPL $EBD5
$EBDA  RTS
```

Ten calls per iteration in both emulators, at the same PPU positions, in the
same order. The whole divergence is which of those waits costs two frames
instead of one.

**Threshold, measured:** entering the wait at scanline 241 costs one frame at
cycle ≤ 67 and two frames at cycle 68. Mesen2 arrives at 50/55/59/62/67/68
across iterations. **We arrive at dot 70 every single time.**

### What is identical, and it is nearly everything

| Measurement | Emu198x | Mesen2 |
|---|---|---|
| CPU cycles between consecutive waits | 24666, 30699, 28122, ... | identical |
| CPU cycles per slow iteration | 357 366 | 357 366 |
| CPU cycles per frame | 29781/29780 alternating | identical |
| OAM DMA stall | 513 | 513 |

Mesen2's fast iteration is 297 804 cycles — exactly 10 frames — so it is not
doing less work, it is losing two fewer frames to the threshold.

### The signature

Mesen2's per-slot cycle counts jitter by **exactly ±7** between iterations —
one poll of the 7-cycle `BIT $2002 / BPL` loop. Ours never jitter by anything.
Our loop is phase-locked into a 12-frame period; Mesen2's visits 10, 11 and 12.

⚠ Our own perfect periodicity is the anomaly, not Mesen2's variation. A
deterministic system that returns to the same state every 12 frames has no
state that fails to return; Mesen2 has one, and it is worth exactly 7 CPU
cycles.

### Acquitted

- **CPU/PPU alignment.** Per-frame CPU cycle counts alternate 29781/29780 in
  both — correct for an 89342/89341-dot pair with the odd-frame dot skip.
- **Frame length.** Identical, strictly alternating, in both.
- **OAM DMA length.** 513 cycles, counted straight off the DMA bus trace: 257
  reads (halt + 256 source reads, no alignment dummy), a 512-cycle span, plus
  the final write cycle.
- **The `$2002` read/VBL race as a simple threshold effect.** The threshold is
  real but sits at scanline 241 cycle 67/68, nowhere near the dot-0-to-2 window
  the documented race occupies.

### ⚠ One wrong claim, made and retracted

Measuring the span `$E50F -> $E2C9` (the `STA $4014` through to the target of
the following `JSR`) gave 524/525 for us against Mesen2's 523/524. Subtracting
the instruction overhead, Mesen2's numbers land exactly on the documented
513/514 and ours land one above — which reads as an OAM DMA one cycle too long,
and `lib.rs` states 513/514 in two comments **with no test ever asserting it**,
so a missing gate around a real defect was entirely plausible.

It is not a defect. Counting the transfer directly off the DMA bus trace gives
513. The span is what is unreliable: both ends are instruction boundaries, but
Mesen2's exec callback fires on opcode fetch while our side triggers when the
PC register takes the value, and for a `JSR` those are different cycles.

**The lesson is a sharper version of one this campaign already carries.** A
cross-emulator comparison is only trustworthy at 1-cycle resolution if both
sides sample the *same event*. The slot-to-slot counts in
`probe_vbl_wait_cpu_cycles` do — they agree exactly, which is itself the
evidence that those measurement points align — and the DMA span does not. When
two instruments disagree by exactly one, suspect the instruments before the
subject.

A second instrument failed the same way and was deleted rather than kept:
`$EBDA` looks like the wait routine's exit, but it is also the not-taken
address of the `BPL $EBD5`, so PC passes through it on every poll. Mesen2's
exec callback marks the real exit; a PC-watch does not. That one reported
"waited 0 frames" for every call, which is at least obviously wrong.

### The mechanism, found

The extra frame is **VBlank suppression**, and it is not a defect.

The wait loop polls `$2002` every 7 CPU cycles = 21 dots. When one of those
polls lands exactly on the moment the VBlank flag sets, the read returns 0 and
consumes the flag, so that frame's VBlank never becomes visible and the loop
waits out another whole frame. Traced directly: at frame 205 a poll's read
lands on the suppression moment, and every later poll that frame reads `$00`
until the next frame's VBlank at 8506 polls — exactly 2.00 frames, against 1.00
for the neighbouring wait.

⚠ **Mesen2 does exactly the same thing.** Its `$2002` reads land on the
suppression moment at frames 154, 158, 166, 170 and 188 — which are precisely
the frames its 12-frame iterations skip. Both emulators lose frames this way.
The difference is only *how often*: our loop is phase-locked into hitting it
twice per 12-frame iteration, and Mesen2's is not.

### ⚠ A second wrong claim, made and retracted

The traces appear to show a behavioural difference. Ours suppresses on a read
logged at scanline 241 **dot 1**; Mesen2 suppresses on one logged at scanline
241 **cycle 0**, and its read at cycle 1 returns the flag SET. Mesen2's source
carries the NESdev rule verbatim — *"Reading one PPU clock before reads it as
clear and never sets the flag or generates NMI for that frame"* — so ours
looked one dot late.

It is not. The two emulators label PPU positions differently: Mesen processes
cycle N and then exposes `_cycle == N`, while we expose the dot we are **about
to** process. Our dot D is the same physical moment as Mesen's cycle D-1. The
suppression windows coincide exactly, and the unit test
`reading_2002_at_241_dot_1_suppresses_vbl` documents our convention.

**Twice in one investigation, a cross-emulator comparison of position or cycle
labels produced a defect claim that direct measurement then refuted** — the OAM
DMA span, and now this. The rule is worth stating plainly: *labels are not
observations.* Compare behaviour anchored to an event both sides agree on, or
compare each side against the documented rule separately.

### ⚠ A third correction: it is 38 frames, not 39

The headline figure was wrong, and by the same mechanism as the other two.

`palette-phases.lua` counts frames with **its own counter**, which starts when
the script loads — after `LoadRom`. Our probe counts from power-on. Comparing
599 against 638 therefore compared two different origins.

Worse, the offset is not even constant: `ppu.frameCount` minus a script's own
counter is **1** when sampled from an `endFrame` callback and **2** from a
mid-frame memory callback, because the script's counter has only been
incremented for completed frames. Two Mesen scripts using different counters
cannot be compared with each other either — which briefly made the suppression
frames look like they matched the skipped frames for the wrong reason.

Re-anchored on CPU cycles, which have no frame-boundary convention:

| | art phase starts | art phase ends |
|---|---|---|
| Mesen2 | 17 865 828 | 37 699 641 |
| Emu198x | 18 997 602 | 38 831 415 |
| difference | **1 131 774** | **1 131 774** |

1 131 774 CPU cycles is **38 frames**. Identical at both ends, which is the
same fact the equal 666-frame durations were showing.

**Three wrong numbers in one investigation, all from the same cause: comparing
labels across instruments that do not share a convention.** The DMA span
compared PC-update against opcode-fetch; the suppression dot compared
about-to-process against just-processed; the headline compared script-load
origin against power-on origin. None of the underlying behaviour was ever
wrong.

The rule earned here: **quote cross-emulator measurements in CPU cycles.**
They count from power-on in both, they have no frame-boundary or
before/after-processing ambiguity, and every comparison in this investigation
that used them — the per-slot counts, the per-iteration totals, the DMA trace —
was correct first time.

### The bisect, and the shape of the whole thing

Done convention-free: match palette transitions by their **value** (which needs
no shared coordinate system at all) and compare the **CPU cycle** at each.

| palette transition | Mesen2 cycle | Emu198x cycle | gap |
|---|---|---|---|
| first write | 57 060 | 27 394 | −29 666 |
| `0F0F0F...` | 116 621 | 116 736 | **+115** |
| ... through frame 16 | | | **+115** |
| `0B1D0D03...` | 533 550 | 503 884 | **−29 666** |
| ... through frame 58 | | | **−29 666** |
| `0F200C2C...` | 13 905 000 | 15 036 773 | **+1 131 773** |
| art phase start | 17 865 828 | 18 997 602 | +1 131 774 |
| art phase end | 37 699 641 | 38 831 415 | +1 131 774 |

The gap does not drift. It sits flat, then **steps by exactly one frame**
(29 781 cycles) at discrete points: once at frame 17 in our favour, then 39
times against us between frames 58 and 466. Net 38.

Every step is one frame, and one frame is exactly what a suppression hit costs.
So the picture is complete:

1. The two emulators start ~115 CPU cycles apart — 0.4% of a frame.
2. The wait loop polls `$2002` on a 21-dot grid. Whether a poll lands on the
   VBlank set dot is decided by where that grid sits, and 115 cycles is enough
   to put the two on opposite sides of it.
3. Each disagreement costs exactly one frame, and shifts the phase again.

**The 38 frames are amplification of a ~115-cycle startup difference, not an
accumulating error.** Both emulators implement suppression identically; they
simply take different numbers of hits.

⚠ This reframes the whole question. There is no 38-frame defect to find. The
only concrete residual is the **115 CPU cycles at startup**, plus the one-frame
offset in when the very first palette write happens — both reset/power-on
timing questions, and both far smaller than what they grow into.

### What remains open

The ~115-cycle startup offset, and the one-frame difference before the first
palette write. Everything downstream is explained.

⚠ Do not use `probe_first_2002_read_per_frame` as a bisect: no frame shift
aligns the two sides (best is 20 identical positions out of 84), so it is not
capturing the same set of reads on both. The palette-value/CPU-cycle method
above is the sound one — it needs no shared frame numbering at all, which is
the whole point.

The ROM passes in both emulators. This is an accuracy question, not a verdict
question, and none of the corpus's gates depend on it.

## ⚠ On acquiring more test ROMs

Candidates for later acquisition once the current corpus is exhausted. Candidates for later
acquisition once the current corpus is exhausted: the NESdev `ppu_sprite_hit`
and `ppu_sprite_overflow` originals, Tom Harte's 6502 per-instruction corpus
(already used for Z80 in this workspace), and `nes_cpu_exec_space`.

## Related

- [`nes-blargg-survey.md`](../processes/nes-blargg-survey.md) — how the
  measurement is taken, and the manifest pinning the ROMs it takes it from.
