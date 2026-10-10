# Decision: C64 accuracy closure campaign

**Date:** 2026-08-08
**Status:** ACTIVE
**Assessment date:** 2026-08-13

## The question

What accuracy work must Emu198x complete before the active Commodore 64 effort
pivots from broad improvement to failure-driven maintenance?

## Current assessment

The strongest supported slice is the PAL breadbin C64 running ordinary disk,
tape and cartridge software. CPU functional coverage, deterministic state and
inspection are strong. The current VIC-II comparison nevertheless contains
repeatable differences in register timing, screen positioning, video modes
and colour-output timing. The far-edge forced-badline C-data path is now
represented by explicit output-delay, counter and carry state, but parity with
mature reference emulators is still not defensible across the behaviour
Emu198x claims to support.

No numerical family score is assigned. The available measurements describe
different assertion boundaries: CPU final-state rows, selected full-machine
tests, representative pixel comparisons, settled SID filter responses and a
small compatibility catalogue. Combining them into one percentage would imply
a weighting that the evidence does not provide.

## Evidence supporting the assessment

### Processor and full-machine behaviour

- The NMOS 6502 comparison passes 2,560,000 of 2,560,000 Tom Harte final-state
  rows and checks per-cycle address, read/write direction and write data. The
  12 JAM/KIL opcodes have documented cycle-trace allowances, and the locally
  supplied corpus revision is not yet pinned in the repository.
- The CPU-only Wolfgang Lorenz subset passes 222 of 222 selected cases. Klaus
  Dormann's functional program reaches `$3469` after 96,241,367 cycles. Both
  are local-fixture lanes rather than default hermetic tests.
- A fresh full-machine Lorenz run passes all 14 independently runnable
  hardware-dependent cases: CIA timers, IRQ, NMI, CPU timing, banking, fetch
  visibility and trap cases. The fifteenth file, `finish`, is the finalizer for
  the suite's chained execution and cannot pass when launched as an isolated
  case. This result must be stated as 14 runnable cases passing, not as a
  14/15 machine-accuracy percentage.

### VIC-II video

The PAL 6569 breadth survey runs 17 selected programs across 13 categories
from the VICE VIC-II testbench and compares each 384 x 272 reference image
after the fixed 16-pixel crop. It registers all five colour-fetch-bug programs
and one representative from each other category. Pixels are classified by
nearest C64 palette index, so the measurement tests digital colour-index
output rather than analogue colour reproduction.

| Category | Matching pixels |
| --- | ---: |
| `spritefetchbug` | 98.211% |
| `vicii_timing` | 99.521% |
| `border` | 100.000% |
| `screenpos` | 100.000% |
| `videomode` | 100.000% |
| `spritecrunch` | 100.000% |
| `sb_sprite_fetch` | 100.000% |
| `gfxfetch` | 100.000% |
| `sequencer-bug` | 100.000% |
| `greydot` | 100.000% |
| `spritedma` | 100.000% |
| `dmadelay` | 100.000% |
| `colorfetchbug` | 100.000% for each of five programs |

These are the frame-routing-version-11 results, after stage 3a, the
mid-line XSCROLL fix (#1620), the mode-bit fix (#1660) and the opened-border
background (#1661). Version 10 measured `border` 93.806% and `vicii_timing`
98.935%, version 9 `vicii_timing` 98.468%, and version 8 `vicii_timing` 96.774% and
`spritefetchbug` 97.226%, with every other row as now. At version 7
the same programs measured `vicii_timing` 84.720%, `screenpos` 87.800%,
`videomode` 88.980%, `border` 92.533%, `spritecrunch` 95.190%,
`spritefetchbug` 97.004%, `sb_sprite_fetch` 98.578%, `gfxfetch` 99.325%,
`sequencer-bug` 99.971%, `greydot` 99.993%, `spritedma` 99.998%,
`dmadelay` and `colorfetchbug` 100%.

These are pixel-match fractions, not test pass rates. Each row is one
representative program except `colorfetchbug`, which reports all five selected
programs. The third-party testbench reference set has uneven per-image
provenance: some images are constructed expectations, some cases describe
measured C64 behaviour, and the set is not uniformly a direct hardware
capture or the output of a second emulator family. The staged corpus upstream
revision is unresolved; the survey runner pins the 37 selected input files by
byte identity. The survey establishes where to investigate; it does not turn
partial matches into conformance claims.

The strict lanes currently require at least 99 percent for PAL 6569
`gfxfetch`, at least 99.9 percent for PAL 6569 `spritedma`, and at least 94
percent overall for NTSC 6567R8 `gfxfetch`. The NTSC residual is concentrated
in the viewport-wrapping rows; overlapping content is approximately 99.3
percent. A separate strict lane requires pixel and indexed-hash identity for
all five PAL 6569 colour-fetch-bug programs, and another for `sequencer-bug`.
`greydot` and `colorsplit` must match exactly. Fixture tests pin CPU store
and opcode cycles in `greydot`, `colorfetchbug`, `sequencer-bug` and
`colorsplit` to VICE x64sc's. (The 99% `gfxfetch` floor predates these
results.) The same colour lane runs `greydot` and `colorsplit` on the PAL
C64C's 8565 against their `-8565` references, and both match exactly. There
is no strict 6567R56A comparison yet.

### SID audio

- The settled-filter oracle contains 410 scenarios across 6581 and 8580. It is
  generated from the reSID implementation vendored with VICE 3.10 using the
  new 8580 filter, and accepts no more than 2 percent or 24 counts of
  peak-to-peak error plus 24 counts of mean error. The current worst results
  are 0.45 percent, 4 counts and 19.9 counts respectively.
- This is filter-response evidence. It does not establish complete oscillator,
  envelope, register-bus or combined-waveform behaviour. The implementation
  and oracle share reSID lineage, and no physical-hardware waveform oracle is
  registered.
- Register-surface behaviour is checked by VICE `testprogs/SID` programs whose
  expected values were measured on real 6581 and 8580 chips, run by the
  env-gated `runtime-commodore-c64` `sid_testprogs` suite: `ringmod`,
  `busvalue`, `osc3-wave0` (both builds) and the CIA-timed `bitfade` delays
  for data-bus hold and TEST noise drift. The drift and floating-DAC timings
  are reSID's; reSIDfp's warm-chip figures differ and the programs' readmes
  report wide chip-to-chip spread, so they bound the behaviour rather than
  pin one chip.
- Combined waveforms and the noise write-back are checked by the same suite
  against real-chip readings (#769). `wb_testsuite` passes 100 of 110
  programs; the ten failures are a strict residual that VICE 3.10's reSID
  shares (it fails 19). `noise_writeback_test1` and the 6581 `wf12nsr`
  build pass; the 8580 `wf12nsr` build differs in two cells, as reSID's
  does. `waveforms` records agreement with gpz's real-chip OSC3 readings
  for each combination; the combined-waveform tables themselves are reSID's
  OSC3 samples (6581 R1/R3/R4, 8580 R5), so that agreement crosses chips
  rather than lineage.
- Waveform-generator timing and power-on state are checked by the same
  suite (#1606): `oscinit` (power-on accumulator and noise register),
  `detect` (the 8580's one-cycle-late OSC3 triangle and sawtooth),
  `osc_topbit` (the 6581's sawtooth-combined MSB pull-down),
  `noise_writeback_test2` (the two-cycle noise shift), `writedelay` (no
  register-write delay on either model) and `noiselfsrinit` (real-8580
  noise-register initialisation) all pass. Against gpz's readings, the 6581
  matches every sample of triangle, sawtooth, pulse, pulse+sawtooth and
  pulse+sawtooth+triangle, and the 8580 every sample of triangle, sawtooth
  and pulse. The hard-sync special case follows reSID and reSIDfp; no
  staged program checks it.

### Determinism and compatibility

- Snapshot envelope version 8 preserves the active VIC-II sprite, fetch and
  render pipeline, queued mixed and per-voice SID audio, the live BA-to-AEC
  handover age, source-resolved bus latches and pending or exhausted far-edge
  badline-window state, the two-cell forced-output delay and the bounded
  12-bit C-data carry. Active-sprite, non-empty-audio, mid-handover, far-edge
  and live-carry regressions compare restored execution with an unforked
  machine. The recursive C64 serde audit currently finds no skipped state in
  the CPU, VIC-II, SID, CIA, board, IEC, drive or runtime stack. The
  live-pipeline foundation was established by commit `6a8cad9c`; the handover
  state was added by commit `9176e269`, and the source/window state by
  `d140a36f`.
- At frame-routing version 7, the 13-entry C64 catalogue retains the
  firmware-only boot, D64, D81, G64, TAP, EasyFlash, Final Cartridge III,
  Action Replay, 1541, 1571 and 1581 matrix. Its hashes are Emu198x regression
  oracles, not independent hardware evidence. Every entry currently uses a
  PAL profile.
- The runtime has PAL and NTSC breadbin profiles plus PAL and NTSC C64C
  profiles. C64C selects the 8580, the 6526A and the HMOS-II VIC-II (8565 PAL,
  8562 NTSC). The VIC-II models the HMOS-II chips' grey dot only; the other
  HMOS-II differences listed under
  [stage D](#d-85658562-chip-axis-and-grey-dot-796) are not modelled.

## Why a parity claim is not yet defensible

### Measured result: late-badline display phase, bus handover and far-edge window

Commit `74f31553` separates the display state entering Phi1 from a forced
badline that becomes active during Phi2. Cycle 16 retains its idle g-access,
the first c-access fills VMLI slot zero, and cycle 17 consumes that slot before
advancing. Rendering now selects the live pre-increment VMLI rather than a
geometry-derived column. An opened vertical border also exposes fresh active
or mode-correct idle output rather than stale framebuffer pixels.

The clean revision-keyed comparison at `74f31553` improved `colorfetchbug` by
24,192 pixels to 92.456 percent and `sb_sprite_fetch` by 23,040 pixels to
98.578 percent. `spritefetchbug`, `border` and `sequencer-bug` also improved;
all eight other indexed output planes remained identical. That revision
settled the focused display-phase question.

Commit `9176e269` then makes bus ownership depend on consecutive aggregate
BA-low cycles. During the first three cycles, before AEC falls, a matrix
access stores `$FF` and the low nibble of an explicit CPU-side Phi2 bus sample
without reading screen or colour RAM. The fourth consecutive BA-low cycle is
the first valid access. The handover does not restart while badline and sprite
DMA causes overlap, and CPU RDY remains driven by BA with the NMOS write-cycle
exception.

All five registered PAL 6569 `colorfetchbug` programs now match every one of
their 104,448 classified pixels and have indexed-plane hashes identical to
their references. The clean report is
`target/accuracy/c64-vicii-survey/9176e2690fe25c069fe2b4cb4529a0de4f22f23d/report.json`.
The selected colour-fetch contract is therefore closed.

The same change moves 136 pixels in `sequencer-bug` and reduces the net match
count by 72, from 96,338 to 96,266. A VICE instruction trace and an Emu198x
pin trace align at the stable-raster handler, through the complete sprite-DMA
stall and at the critical `$3B` write after normalising their observation phases.
The critical trigger is therefore not an upstream CPU timing lead. Continuing
the trace reveals a separate defect after it: Emu198x holds the next opcode
for 21 cycles across the forced-badline and sprite interval, while VICE's
instruction schedule implies the ordinary 19-cycle sprite interval. The
defect was classified as late-created fetch-window termination followed by
delayed C-data output sequencing. Reverting the now-exact ownership rule would
conceal those separate omissions.

Commit `d140a36f` retains the CPU completion phase of the critical `$D011`
write and gives a cycle-53 far-edge transition exactly one remaining c-access.
An exhausted explicit window cannot reopen through the ordinary cycle-54
predicate. Source-resolved traces now show the sole badline access, the
following scheduler-phase gap and the independent sprite BA source; the next
store aligns with VICE at cycle 55.

`sequencer-bug` improves by 8,128 pixels to 104,394 of 104,448, or 99.948
percent. Every other registered indexed plane is unchanged, including all
five exact colour-fetch cases. The clean report is
`target/accuracy/c64-vicii-survey/d140a36f782862706e04b15272bf5f7f4a145862/report.json`.
The remaining 54 pixels occupy eight reference rows and are classified as the
delayed C-data output question. Three documents record the independent
questions: [PAL 6569 late-badline display phase](c64-late-badline-display-phase.md),
[C64 BA-to-AEC handover](c64-ba-aec-handover.md) and
[PAL 6569 far-edge late-badline DMA window](c64-far-edge-badline-window.md).

The 2026-08-13 C-data correction preserves the two cells already resident in
the output path, suppresses VC/VMLI only for the first following idle
g-access, then applies Hoxs64's bounded 12-bit carry network on eligible
RC-zero output. The second hidden cell is backed by an active g-access and
advances both counters. It is the only indexed survey plane whose hash
changes in the full comparison; all five colour-fetch cases remain exact.
`sequencer-bug` rises from 104,394 to 104,418 matching pixels, closing 24 of
the historical 54 disagreements.

The remaining 30 pixels consist of two dot-zero colour-register transitions
and one 8 x 8 character outline containing 28 foreground pixels. The two dots
belong to the unimplemented PAL 6569 colour-resolution ring, which
[Stage 3a](#stage-3a-re-phase-the-raster-edge-796) traces to the raster-edge
phase. The outline is
the compressed direct renderer's unresolved separation between active
g-access/counter state and delayed visual output. An experiment that
suppressed both hidden counter advances reached 104,446 pixels but was
rejected: Hoxs64 advances the active g-access behind the second visually
hidden cell. The C-data and counter-state decision is
[PAL 6569 far-edge forced-badline C-data](c64-forced-badline-cdata-pipeline.md).

### Other claim boundaries

One C64 machine tick advances exactly one Phi2 cycle. No double-tick or
overtick was found in this investigation. The VIC-II still renders its eight
pixels as a batch, and CPU register writes become visible after that batch.
The model therefore has no explicit dot or half-cycle colour-resolution
contract for cases that change a register-backed colour during those eight
pixels. The retained `sequencer-bug` output signature and the
`vicii_timing` residual make that boundary material rather than theoretical.

Other claim boundaries remain:

- CIA timer and interrupt behaviour is well exercised. Since #797 the serial
  port follows VICE's output pipeline and shifts input on CNT, CNT clocks
  the timers, and CNT1/SP1/CNT2/SP2, PC2 and FLAG2 are user-port pins a host
  device drives. Nothing drives them on a bare machine, and serial-bus SRQ
  (CIA1 FLAG on the C64) stays unattached: only the C128's fast serial uses
  it, and there is no C128.
- The SID's waveform generator follows reSID 1.0's cycle-exact path: open-bus
  decay, TEST drift, ring-modulation polarity and the floating DAC input
  since #777; combined waveforms for both models, the noise taps, the
  noise+pulse pull-down and the combined-waveform noise write-back since
  #769; the two-cycle noise shift, the one-cycle pulse compare, the 8580's
  late OSC3 triangle and sawtooth, the 6581's sawtooth-combined MSB
  pull-down, the hard-sync special case and the power-on state since #1606.
  8580 register writes are deliberately not delayed: reSID delays them only
  in its non-cycle-exact mode, as a stand-in for the OSC3 delay, and
  `writedelay` measures none. Two residuals stay against real chips, as
  they do for VICE's reSID: ten `wb_testsuite` programs, seven of them
  6581 releases into noise+pulse, where writing back the value before the
  noise+pulse pull-down fixes four but breaks the 6581 `wf12nsr` build;
  and two 8580 `wf12nsr` cells, noise+pulse over a full register, which
  read `$FC` (reSID's value) where the chip reads `$F8`. reSIDfp's guessed
  noise+pulse model gives `$FE`, so neither reference reaches it.
- Ultimax unmapped reads do not yet model the required open-bus behaviour.
- Invalid matrix accesses deliberately do not update the simplified
  `last_bus_data` latch. The effect of disconnected Phi2 activity on that
  latch remains an evidence-bounded open-bus question.
- Sprite Phi2 bytes 0 and 2 are not yet governed by a selected external oracle
  for their AEC-sensitive invalid-access sideband.
- REU transfers complete as one machine operation rather than participating in
  cycle-visible bus arbitration.
- The catalogue has no NTSC or C64C entry.
- The VIC-II differential is retained as a revision-keyed report with pinned
  selected assets. The staged testbench's exact upstream revision and the
  per-image evidence provenance remain unresolved. The VICE 3.10 source
  holding is identified by release, but its exact upstream source revision has
  not been recovered.
- The Lorenz corpus provenance is not pinned in-tree, and the full-machine
  harness does not yet reproduce the suite's chained `finish` semantics.
- D64, D71, D81, G64 and live 1541/1571/1581 paths have directed or catalogue
  evidence, but this campaign does not treat one successful title as format or
  drive-mechanism completeness.

## Ordered closure campaign

Work proceeds in this order:

1. Preserve the snapshot/replay and catalogue foundation. No timing change may
   weaken the active-sprite, queued-audio, byte-fixed-point or 13-entry replay
   gates.
2. Preserve the revision-keyed VIC-II report, including fixture identity,
   model, crop, palette-classification method and exact per-case results.
3. Preserve the exact five-program forced-badline c-access contract. Revision
   `d140a36f` closes the far-edge fetch-window length, and the 2026-08-13
   correction closes the bounded C-data and hidden-output counter state. Next model
   explicit separation between the active g-access/counter stage and delayed
   visual output required by the 28-pixel `sequencer-bug` outline. The
   colour-resolution ring and the `videomode` lead are one fault, the
   raster-edge phase; [Stage 3a](#stage-3a-re-phase-the-raster-edge-796)
   plans it. Then address `vicii_timing`. Introduce explicit dot or
   Phi1/Phi2 stages where the evidence requires them. A change must improve
   the targeted oracle without absorbing an unexplained regression in a
   stronger lane. Treat the testbench program and reference image as the
   black-box contract, inspect vendored VICE 3.10 as implementation evidence,
   and use Hoxs64, VirtualC64 or MiSTer where they can independently classify a
   residual. VICE source is not itself the specification.
4. Promote corrected representative cases to strict assertions and broaden
   within each category before making a category-level claim. Add strict
   6567R56A and further 8565 contracts only after suitable model-specific
   references are registered. Stage 3a-D added the first 8565 contract
   (`greydot` and `colorsplit`).
5. Expand SID verification beyond the shared-lineage filter oracle. Record the
   allowed deviations for oscillator, envelope, register-bus and combined-wave
   behaviour, and prefer an independent implementation or hardware capture
   where one can answer the question.
6. Add selected NTSC and C64C catalogue entries. Re-run the breadbin/C64C,
   PAL/NTSC, media, snapshot and audio matrix at the resulting revision.
7. Pin the CPU and Lorenz corpus identities and reproduce the Lorenz chained
   finalizer semantics. Preserve a machine-readable result rather than relying
   on a terminal transcript.
8. Re-run every declared gate and classify every remaining disagreement as
   fixed, explicitly outside the supported claim, or blocked on stronger
   evidence.

Bus, CIA, drive or peripheral work enters this campaign only when it is needed
by a selected comparator or catalogue failure. Each implementation change is
committed separately from evidence requalification.

## Stage 3a: re-phase the raster edge (#796)

Approved 2026-10-06. This stage replaces the "colour-resolution ring" and
"`videomode` phase-accounting lead" items in step 3 above. Both turned out to
be one fault, and it reaches further than colour.

### Finding

Two timing errors cancel in most test programs:

- **The CPU sees the raster-line edge 2 cycles early.** Emu198x raises the
  raster IRQ for the CPU access of engine cycle 62, and `$D012` (and `$D011`
  bit 7) report the new line from that same access. VICE x64sc increments the
  line and raises the IRQ in the Phi2 half of cycle 1 of the new line
  (`viciisc/vicii-cycle.c`, `vicii_cycle`, "Handle end of line" and "Trigger
  a raster IRQ"). With engine cycle N mapped to VICE cycle N, as the rest of
  this crate does, the CPU sees the edge at cycle 62 instead of cycle 1.
- **Colour-register writes reach the screen 2 cycles late.** Emu198x resolves
  the colour registers for cell T with the writes made up to CPU cycle T-1.
  VICE draws cell T in draw cycle T+1. That is where its border checks
  (`ChkBrdL1` at cycle 17, `ChkBrdR1` at 57) land, given a one-draw
  `border_state` lag in `draw_border8`. Dots 1-7 of that cell are resolved in
  the next draw, with writes up to cycle T+1. On the 6569, dot 0 is resolved
  one draw earlier, with writes up to cycle T (`draw_colors_6569`).

In a program timed by the raster IRQ (`greydot`, `colorsplit`,
`sequencer-bug`), the early CPU and the late output cancel, so
every image matches. In a program timed by a CIA timer, the timer read absorbs
the IRQ error, so the CPU runs at VICE's phase. `colorfetchbug` is one: its
`$D011` store sits at cycle 16 and its `inc $d020` store at cycle 55 in both
emulators. There the late output is visible only where a colour change meets a
cell boundary. Until now the border hid it. A dot-0 colour rule exposes it, so
no first-dot rule can be right in the current timing. A 6569 rule that makes
`greydot` exact leaves 51 wrong pixels in each of the five `colorfetchbug`
programs, all at the first right-border dot.

The `sequencer_bug_d011_write_cycle_boundary` diagnostic records the same
offset. The first `$D011` store has Emu198x pins at c52 and a VICE watchpoint
at c54. It treats the gap as two observation conventions. The finding above
shows it is a real 2-cycle lead of the CPU over the VIC-II.

### Measurement on the prototype

A throwaway prototype was built on revision `92892eaf`. It moved the
CPU-visible line edge and the IRQ 2 cycles later and resolved colours two
ticks after rendering, with the VICE dot-0 rule. The patches are kept outside
the repository at `~/.emu198x/wip/796-vicii-raster-phase/`. They use
environment hooks and are not mergeable. PAL 6569 survey, as matching
pixels:

| Case | Main | Prototype |
| --- | ---: | ---: |
| `screenpos` | 87.800% | 100.000% |
| `videomode` | 88.980% | 100.000% |
| `vicii_timing` | 84.720% | 86.929% |
| `border` | 92.533% | 92.548% |
| `sb_sprite_fetch` | 98.578% | 98.632% |
| `gfxfetch` | 99.325% | 99.571% |
| `greydot` | 99.993% | 100.000% |
| `colorfetchbug` (all five) | 100.000% | 100.000% |
| `spritedma` | 99.998% | 99.998% |
| `spritecrunch` | 95.190% | 95.190% |
| `dmadelay` | 100.000% | 89.103% |
| `sequencer-bug` | 99.971% | 92.264% |
| `spritefetchbug` | 97.004% | 92.671% |

The three regressions all depend on when a CPU write reaches the badline or
sprite-DMA logic. Their models were fitted against the early CPU, so they now
need re-deriving. The far-edge `$D011` window keys on recorded cycle 53 or
later, and that cycle moves by 2.

### Reproducing the evidence

The vendored source is `../../../../emulators/c64/vice-3.10/`. The Homebrew
`vice` 3.10 build of `x64sc` runs headless:

```sh
x64sc -default -console -silent -sounddev dummy -VICIImodel 6569 \
  -VICIIborders 0 -VICIIfilter 0 -warp -limitcycles 12000000 \
  -exitscreenshot out.png -autostartprgmode 1 -autostart <program.prg>
```

The screenshot is 384 x 272, the same window as the references. Its palette
differs, so classify its colours by majority vote against the reference at
the same positions. On 2026-10-06 this reproduced the `greydot` (6569 and
8565), `colorsplit` (6569 and 8565), `sequencer-bug` and `colorfetchbug-main`
references with no disagreement.

To time stores, add `-moncommands mon.txt` with:

```text
logname "mon.log"
log on
trace store d021
x
```

The monitor prints `line/cycle` from `maincpu_clk` (`c64/c64.c`,
`machine_get_line_cycle`), not from the VIC-II raster. In practice the two
agree: in the `greydot` run, `lda $d012` at monitor line 205 reads `$CD`.
(An earlier version of this record said that run read 96 lines off. That
compared the raster IRQ's line with a different dot row's.) The
`colorfetchbug` readme anchors the cycle: the program writes vscroll in cycle
15 counted from zero, and the monitor reports the store at 16. A store
therefore appears at the engine cycle that writes it, and an opcode at the
cycle before the engine cycle that fetches it.

On the Emu198x side, `cpu_store_cycle_boundary` in
`crates/runtime-commodore-c64/tests/vicii_testbench.rs` lists every CPU write
to one register (`VICII_STORE_ADDR`, `VICII_STORE_PRG`) with its engine cycle.

### Stages

Every stage is its own PR, in this order. The family is rebase-merge only, so
each PR's commits survive.

**Merge unit.** A, B and C cannot land one at a time without regressing a
strict lane, so they merge together:

- A alone moves every raster-timed program's writes 2 cycles later while the
  output stays 2 cycles late. The `sequencer-bug` exact signature and the
  colour splits break.
- C alone resolves colours 2 cycles early for raster-timed programs. The
  prototype with C only took `colorsplit` from 1,008 to 1,960 disagreements
  and changed the `sequencer-bug` signature.
- A with C still regresses `sequencer-bug`, a strict lane, until B lands.

So A, B and C are opened as three stacked PRs and reviewed one at a time.
They merge in one sitting, once C is green on every gate below, retargeting
each to `main` before its base merges. Only the merged result has to leave
every strict lane at least as good as main. Each stacked PR states the lanes
it leaves red and why. D depends only on C and merges on its own afterwards.

#### A. CPU-visible line edge, raster IRQ, `$D011`/`$D012` reads

Make the CPU's view of the line edge part of the model: a line number that
changes in cycle 1, read by `$D011`, `$D012` and the raster compare. Assert
the raster IRQ in VICE's phase: cycle 1, or cycle 2 on line 0, where
`vicii_cycle_start_of_frame` runs. Write the convention down as one named
mapping in `Vic`, not as offsets scattered through the tick.

**As built (amended 2026-10-06):** the badline comparator and the vertical
border flip-flop read the same raster counter, so they also move to cycle 1.
Engine cycle 0 is VICE's cycle 63 of the previous line, and VICE's
`check_badline` and `check_vborder_*` run there with the old line. Keeping
them on the engine's line number failed `dmadelay`: once its `$D011` stores
landed at VICE's cycles, the store at engine 0 of line 48 made that line a
badline and moved the whole screen up a character row.

- Files: `crates/mos-vic-ii/src/lib.rs` (`tick` IRQ assertion, `read` and
  `peek` for `$11`/`$12`, `check_badline`, the vertical border flip-flop, the
  raster-IRQ and border unit tests); `crates/machine-commodore-c64/src/machine.rs`
  (its IRQ test); `crates/runtime-commodore-c64/tests/vicii_testbench.rs`
  (the VICE store-phase gate and the generalised write diagnostic). The
  `sequencer-bug` diagnostic moves to stage B, because that program's phase
  also depends on sprite DMA.
- State: derived from `raster_line` and `raster_cycle`, so no snapshot change.
  If a latched compare state turns out to be needed, it bumps the snapshot.
- Gates:
  - A new unit test asserts that the CPU sees the IRQ on engine cycle 1, and
    that `$D012` reads the old line on engine cycles 62 and 0. It fails on
    main.
  - A new fixture test, `cpu_store_phases_match_vice`, asserts `greydot`'s
    `$D021` stores at engine cycles 17, 21, ... 53, VICE's phase, and that
    `colorfetchbug`'s CIA-timed stores stay at 16 and 55/61. It fails on main,
    which stores `greydot`'s at 15.
  - Lorenz full-machine cases: all 14 runnable cases still pass.

#### B. Re-derive the `$D011`, far-edge, C-data and sprite-DMA timing

With A in place, trace `dmadelay`, `sequencer-bug` and `spritefetchbug`
against VICE. Use the method above, one run per program, anchored to a known
store. Re-derive the far-edge window threshold (recorded cycle 53), the
pending `$D011` completion phase, the two hidden output cells and the 12-bit
C-data carry origin, and the sprite-DMA and `$D017` write phases. Keep each
retained rule only where the VICE trace still supports it.

- Files: `crates/mos-vic-ii/src/lib.rs` (`check_badline`, far-edge window,
  forced-output delay, C-data carry, sprite-chain write phases);
  `crates/mos-vic-ii/src/sprite_fetch_chain.rs` if crunch timing moves;
  `crates/runtime-commodore-c64/tests/vicii_testbench.rs` (the
  `sequencer-bug` signature); the three records named below.
- State: if the fields change shape, bump `SNAPSHOT_VERSION` in
  `crates/runtime-commodore-c64/src/snapshot.rs`.
- Gates:
  - `colorfetchbug`: all five programs stay pixel- and hash-exact.
  - `dmadelay` returns to 100.000%.
  - `sequencer-bug` gets an exact retained signature no larger than main's
    30 pixels.
  - `spritefetchbug` is at least 97.004%.
  - `spritedma` is at least 99.9%, and PAL `gfxfetch` at least 99.325%, which
    replaces the old 99% floor.
  - The revision-keyed survey report shows no other indexed hash change
    without an entry in the progress log.

#### C. Two-tick colour resolution

Render symbolic colour sources (`$00`-`$0F` direct, `$20`-`$2E` registers) as
now. Resolve each cell two ticks later, when the writes of the following two
CPU cycles are visible. On the 6569, dot 0 is resolved one tick earlier,
without the latest write. This is the general colour-ring contract the C-data
record asks for, not a rule for one register. Flush the two pending cells at
the frame edge so a captured frame is complete.

**As built (amended 2026-10-06):** the side border moved into the same stage.
VICE checks the main border in cycles 17/18 and 56/57 and draws a cell one
cycle after its checks (`draw_border8`'s `border_state`), so a CPU write in
the cycle before a check still decides it. The engine's border flip-flop ran
a cycle earlier. Only the 2-cycle-late output had kept the side-border
tricks in `spritefetchbug` working. The colour stage now applies VICE's
border, from its last two states, when it resolves a cell, and the border
covers sprites, as `draw_border8` does after `draw_sprites8`. Outside the
fetch window the sequencer shifts out zero graphics instead of leaving the
previous frame's pixels, which an opened side border exposes.

- Files: `crates/mos-vic-ii/src/lib.rs` (`render_pixels`, a pending-cell ring,
  `Vic::write` recording the colour write, `FRAME_ROUTING_VERSION` 7 → 8 with
  its doc entry); `crates/runtime-commodore-c64/src/snapshot.rs`;
  `crates/emu198x-catalogue/manifest/c64.toml`;
  `crates/runtime-commodore-c64/tests/vicii_testbench.rs`.
- State: the two pending cells and the last colour write are live pipeline
  state. Bump `SNAPSHOT_VERSION`, and add a mid-cell snapshot round-trip
  regression to the existing replay gates.
- Gates:
  - `colour_register_pipeline_matches_each_chip_reference`, written for this
    stage. `greydot` is exact on the 6569. `colorsplit` keeps only the 952
    disagreements on its 16 XSCROLL rows, where the renderer latches XSCROLL
    once per line. It fails on main, which has 7 and 1,008.
  - Every strict lane at least as good as main.
  - Survey: `screenpos` and `videomode` at 100.000%.
  - Catalogue: re-capture all 13 C64 entries at routing version 8 with
    `catalogue capture`. Each changed frame hash is listed in the PR. All
    entries must then pass ordinary and fresh-runtime replay. As a check that
    can fail, run the replay before re-capture and confirm the version
    mismatch fails loudly.

#### D. 8565/8562 chip axis and grey dot (#796)

Add `VicModel::Pal8565` and `VicModel::Ntsc8562`, with 6569 and 6567R8
timing, and map `PalC64c` and `NtscC64c` to them in
`crates/machine-commodore-c64/src/machine.rs`. On these chips, dot 0 of the
cell after a colour-register write shows light grey (`$F`) when its source is
the written register (`draw_colors_8565`). This closes #796.

- Files: `crates/mos-vic-ii/src/lib.rs`,
  `crates/machine-commodore-c64/src/machine.rs`,
  `crates/runtime-commodore-c64/src/snapshot.rs` (the stored revision flag),
  `crates/runtime-commodore-c64/tests/vicii_testbench.rs`.
- Gates:
  - `greydot` on the 8565 is exact against `greydot.prg-8565.png`. It fails
    on main, with 400 missing dots.
  - `colorsplit` on the 8565 keeps only its XSCROLL-row signature.
  - All PAL breadbin lanes and catalogue hashes are unchanged. Every catalogue
    entry is a breadbin, so no re-capture is expected. A changed hash fails the
    PR.

**As built (amended 2026-10-06):** the `Vic` stores its chip revision, so
snapshot version 12 carries it. `colorsplit` on the 8565 keeps 960
disagreements, eight more than the 6569's 952. All eight are at x = 192 on
the XSCROLL rows: four rows show a grey dot the reference lacks and four lack
one it shows, because those rows keep the previous test's scroll.
`FRAME_ROUTING_VERSION` stays at 8, because no breadbin output changes and
the catalogue has no C64C entry.

A diagnostic run of the testbench programs that carry a `-8565` reference
(2026-10-06, not a gate) compared 36 of them; the harness cannot decode the
other two references, which are indexed-colour PNGs. Every breadbin result
was unchanged. On the C64C, no program moved further from its 8565
reference, and 13 now match it exactly where none did before: `greydot`,
`screenpos`, `sequencer-bug`, `rmwtest`, `lp-trigger`, four `spriteenable`
and four `spritesplit` programs. Besides `colorsplit`'s eight XSCROLL dots,
two programs are further from the 8565 reference than the breadbin is from
the 6569's: `ss-hires-mc` and `ss-hires-mc-exp`, by 44 pixels each.

VICE's `color_latency` flag covers more than the grey dot. These HMOS-II
differences were outstanding when the chip axis was introduced:

- the sprite multicolour flag is updated at dot 6, not dot 7
  (`update_sprite_mc_bits_8565`), the likely cause of the two `spritesplit`
  residuals above;
- the `$D011` mode bits enter the graphics pipeline once per cycle rather than
  on rising and falling edges at dots 4 and 6 (`vmode11_pipe`);
- an idle graphics fetch reads the delayed `$D011` (`vicii_fetch_idle_gfx`),
  and the 6569's mixed-mode address latch (`vicii_fetch_graphics`) does not
  apply;
- the light pen latches one X unit earlier (`x_extra_bits`), corrected in the
  bounded light-pen work below.

**Light pen (#1630 item 4, 2026-10-10).** The chip selects one extra X unit
on HMOS-II and two on NMOS, and the held-low frame retrigger now runs when the
raster counter enters line zero. Running that event at engine wrap previously
fed the last line into the suppression rule and lost the retrigger. Existing
state is sufficient; the snapshot layout is unchanged.

The full 256-delay measurement matches native VICE and the prepared physical
references in all 5,120 bytes on four models (594 native disagreements before).
The upstream Makefile converts pre-R03 `.prg` dumps with `makeref.c` before
embedding them in the R04 guest. Its documented correction changes just LPX
samples 254/255 in these inputs; its complete output matches both native VICE
and the guest's embedded references. No samples are excluded. This establishes
parity for the measured schedule, not every possible light-pen condition.
The provenance and observation boundary are in the
[shared reference record](../../../../reference/by-topic/vic-ii/2026-lightpen-measurement-observations.md);
`test-data/commodore/c64/lightpen/` holds the native fixtures. All ten strict
VIC-II lanes and the frame-boundary snapshot checks pass. The full catalogue
remains a merge gate while its external media volume is unavailable.

**NTSC (8562).** `greydot` has two stable launch phases in native VICE
x64sc 3.10 on both the 6567R8 and 8562. Its PAL stabiliser does not remove
the one-cycle phase difference on a 65-cycle NTSC line. The first `$D021`
store lands at raster 110, cycle 15 or 16; later bands alternate phases.
Comparing the two native 8562 images reproduces exactly the 521 pixels
previously attributed to an emulator timing fault in #1629. No production
CPU or VIC-II change is needed for that discrepancy.

The `ntsc_greydot_matches_both_native_launch_phases` regression checks six
BASIC launch times per NTSC model, requires both phases, and compares all
408 stores and 94,848 reference-window pixels for each run. Every observation
matches its corresponding native phase; cross-comparing phases retains the
521-pixel 8562 negative control (49 on the 6567R8). Fixtures are under
`test-data/commodore/c64/ntsc-greydot/`; the [native capture and evidence](https://github.com/emu198x/docs/blob/main/plans/2026-10-10-c64-ntsc-greydot.md)
record the explicit ROM, palette and launch settings. This establishes
software-reference parity for the measured phases, not physical-hardware
validation or complete NTSC coverage.

`colorsplit` matched the 8562 reference except its XSCROLL rows when measured
before #1620; those NTSC rows still need a fresh comparison. On the 6567R8,
the earlier comparison also differed in four eight-pixel blocks where VICE's
window wraps the frame.

### Risks

- **NTSC coverage is partial.** The prototype measured PAL only. A and C
  change the 6567R8, 6567R56A and 8562 paths too. Keep both native `greydot`
  phases exact and re-run `ntsc_gfxfetch_matches_vice_reference` (at least
  94%) at every stage. Refresh the NTSC `colorsplit` comparison before
  claiming it. Line 0's late IRQ and the 6567R56A cycle table need checking
  separately.
- **The snapshot version is shared.** `SNAPSHOT_VERSION` in
  `crates/runtime-commodore-c64/src/snapshot.rs` covers every C64 chip. The
  SID work in #769 runs in parallel and may bump it too. Rebase onto whatever
  lands first and take the next free number. Do not edit SID files from this
  campaign.
- **Catalogue re-capture.** A changes IRQ timing for every C64 program, so
  frame hashes and possibly audio hashes move. A changed audio hash comes from
  CPU timing, not SID routing, so `AUDIO_ROUTING_VERSION` stays with the SID
  crate. Record the cause in `c64.toml` and coordinate with #769 if both
  re-capture at once.
- **Fitted models may not come back.** B may find that a retained rule from
  the far-edge, C-data or BA-to-AEC records was fitted to the early CPU and
  has no support at the corrected phase. The record is then amended in B's
  PR, per "hardware reality beats the record".
- **XSCROLL is a separate fault.** `colorsplit` keeps 952 disagreements,
  because the renderer latches XSCROLL once per line. That is outside this
  stage. (Fixed by #1620; see the next section.)

## Mid-line XSCROLL (#1620)

### Finding

The graphics sequencer reloads its shift register after every g-access,
delayed by XSCROLL dots (Bauer, section 3.7.3). The renderer latched XSCROLL
once per line, at cycle 16, so a mid-line `$D016` write waited for the next
line. VICE x64sc applies it at the next load (`viciisc/vicii-draw-cycle.c`,
`draw_graphics8` and `draw_graphics`):

- At the end of draw cycle N, VICE moves the g-access of cycle N-1 into
  `gbuf_pipe1_reg` and samples `xscroll_pipe` from `$D016`. Draw cycle N+1
  loads that data into the shift register at dot `xscroll_pipe`.
- `vicii_cycle` draws after the CPU access of the previous cycle. So the
  XSCROLL a cell loads at is the one that stands after the CPU access of the
  cell's own g-access cycle. A write in cycle N moves column N-16 onwards.
- Dots before the load show the previous cell's remaining pixels, then zero
  bits. A larger XSCROLL leaves a gap of zero-bit colour; a smaller one cuts
  the previous cell short.
- `xscroll_pipe` is sampled only while the fetch window is open, and VICE
  has closed it by the sample for column 39. A write in cycle 55 therefore
  misses column 39. VICE's own comment calls that gating a convenience;
  Emu198x follows it anyway.

The engine renders a cell before the CPU access of its cycle, so `Vic`
keeps the load it just made, and a `$D016` write in the same cycle re-loads
that cell. `colorsplit` stores `$D016` in cycle 21 in both emulators (VICE
`trace store`), which moves column 5 onwards.

The re-load recomposites the cell's sprites. The cycle's sprite-background
collisions keep the foreground from before the write; no program in the
survey depends on that cycle. (#1660 replaced the re-load: the sequencer now
draws each cell as it leaves the colour stage. See
[Mode bits at the shifter](#mode-bits-at-the-shifter-1660).)

### Measurement

- `colorsplit`: 952 disagreements to 0 on the 6569, and 960 to 0 on the
  8565. It cannot tell the two phases apart: sampling XSCROLL when the cell
  renders, one cycle late, also gives 0.
- `modesplit` (`split-tests`) does tell them apart. Against VICE x64sc 3.10's screenshot it has 7,106
  disagreements on main, 6,086 when sampled at render, and 5,990 at VICE's
  phase. Its `$D016` stores agree with VICE's cycle for cycle (11, 19, 25 and
  44). The rest is mode bits; see below.
- Unit tests pin the phase: a write in column 5's g-access cycle moves
  column 5, a smaller XSCROLL cuts the previous cell short, and a write in
  cycle 55 misses column 39. Each fails without the re-load or the cycle-55
  rule.
- Survey: `vicii_timing` 96.774% to 98.468% and `spritefetchbug` 97.226% to
  98.211%. Every other indexed hash is unchanged, `border` included.
  `vicii_timing`'s gain is its "open border with" rows, which change
  `$D016` with read-modify-write instructions, and its `BC0`-`BC3` text.

### What remains

(#1661 resolved `border` and the side-border rows below; see
[Opened border background](#opened-border-background-1661).)

`border` (93.806%) does not share the cause. VICE x64sc reproduces its
reference exactly, and Emu198x differs from VICE by 6,470 pixels, all on the
lines below the display window where the program has opened the side
border. There the vertical border is set and the display state is idle.
VICE keeps loading zero graphics with the last matrix data, which the idle
lines before left at zero, so hires bitmap mode shows black. Emu198x fills
the display column with `$D021`.

`vicii_timing` (98.468%) differs from VICE by 1,584 pixels (VICE differs
from the reference by 7):

| Rows | Pixels | Cause |
| --- | ---: | --- |
| `BC0`-`BC3`, right side border | 525 | The opened side border shows the last character's ECM background ("continuation of last character bg", per the program's source). VICE keeps the last matrix and colour entries; Emu198x uses zero ones. Same class as `border`. |
| Sprite X-position timing | 336 | Mid-line sprite X writes. |
| X-expand | 157 | Mid-line `$D01D` writes. |
| `MCM`, `BMM`, `ECM`, `E+B` | 550 | Mode-bit changes mid-cell (and the same side-border fill at the right edge). |
| `INC` | 16 | Unclassified: light grey in VICE, orange in Emu198x, at x = 352-358. |

The mode bits are a separate fault, visible in `modesplit`, fixed by #1660;
see [Mode bits at the shifter](#mode-bits-at-the-shifter-1660).

Not modelled, with no program to check it against: with the side border
open and XSCROLL above 0, column 39's last pixels should spill into the
border, followed by zero bits. (Since #1661 the sequencer runs outside the
display window and shifts them out; no program checks it yet.)

## Mode bits at the shifter (#1660)

### Finding

The display mode belongs to the graphics data sequencer, whose heart is an
8-bit shift register reloaded after every g-access, delayed by XSCROLL dots
(Bauer, section 3.7.3). The renderer decoded each cell's mode as it rendered
the cell, before the XSCROLL shift, so a mode boundary moved with the
graphics. `modesplit` steps XSCROLL through 0-7, one value per character row,
and switches modes at fixed cycles. In VICE x64sc 3.10 its black invalid-mode
blocks have straight vertical edges, as they do in the 6569R5 capture in the
testbench (`split-tests/modesplit/screenshots`, taken from an earlier revision
of the program). In Emu198x they stepped one dot per line.

VICE (`viciisc/vicii-draw-cycle.c`, `draw_graphics8`) keeps the raw bits in
the shift register with the matrix and colour entries loaded beside them, and
decodes each dot with the mode as it stands then:

- MCM reaches the colour lookup at dot 4.
- On the NMOS 6569, ECM and BMM rise at dot 4 and fall at dot 6
  (`vmode11_pipe |=` then `&=`).
- On the HMOS-II 8565 they arrive after dot 7.

The issue had the two chips the other way round. `color_latency`, which
selects the 6569 path, is 1 for the 6569R1 and R3 and 0 for the 8565
(`viciisc/vicii-chip-model.c`).

The draw happens two cycles after the g-access. The XSCROLL and the mode bits
for dots 0-3 are sampled after the CPU access of the cell's own cycle, and
the mode bits for dots 4-7 after the next cycle's access. The engine's colour
stage already holds each cell for two ticks, so the sequencer now draws a
cell as it leaves the stage, from its g-access and the registers as they
stand. A mode write in column N's g-access cycle therefore reaches column
N-1 from dot 4 on the 6569, and column N on the 8565. An XSCROLL write in
that cycle still moves column N, as #1620 found. Sprites are composited at
the same point. Collisions are still accumulated as the cell renders; they
use the foreground the cell would have if neither of the next two CPU
accesses changed `$D011` or `$D016`.

The g-access address reads some mode bits late as well (VICE
`vicii_fetch_graphics`). The 8565 addresses with `$D011` as it stood for the
previous g-access. The 6569 sees ECM and a rising BMM at once and a falling
BMM a cycle late. When a BMM change turns a RAM fetch into a character-ROM
fetch, the 6569 takes the low address byte from the old address and the rest
from the new. Bauer covers neither. Both left late cells after some of
`modesplit`'s mode changes; without the ROM rule the 6569 keeps all of its
residual.

The invalid modes now fetch their graphics too, with ECM holding address
lines 9 and 10 low (Bauer, section 3.7.3). A later mode change can reveal
those bits, and their foreground pixels set sprite priority and collisions
(Bauer, section 3.7.3.6).

Foreground now follows VICE's `pixel_pri = px & 2`: a multicolour "01"
pair counts as background for sprite priority and collisions, as Bauer
says for both multicolour modes (sections 3.7.3.2 and 3.7.3.4). The old
renderer counted any non-zero pair as foreground, which hid a
background-priority sprite over a "01" pair. That is the one C64 catalogue
change: two pixels of Aztec Challenge's in-game scene, where such a sprite
lies over a `$D022` pair. Reverting only that rule restores the old hash.

### Measurement

Disagreements with VICE x64sc 3.10's screenshot (`-VICIIborders 0`), by
classified colour index:

| Program | Chip | Main | Decode at the shifter | And the late g-access mode |
| --- | --- | ---: | ---: | ---: |
| `modesplit` | 6569 | 5,990 | 190 | 36 |
| `modesplit` | 8565 | 5,800 | 708 | 0 |
| `vicii_timing` (`-a5`) | 6569 | 1,593 | 1,161 | 1,105 |
| `vicii_timing` (`-a5`) | 8565 | 1,599 | 1,253 | 1,105 |
| `colorsplit` | both | 0 | 0 | 0 |
| `border` (`-250`) | both | 6,470 | 6,470 | 6,470 |

- Phase, before the g-access change: drawing one cycle earlier, with dots
  4-7 read after the cell's own CPU access, left `modesplit` 3,748 from VICE
  on the 6569; drawing as the cell renders left 6,963.
- `vicii_timing`'s `MCM`, `BMM`, `ECM` and `E+B` rows go from 450
  disagreements to 0.
- Survey: `vicii_timing` 98.468% to 98.882% at the shifter and 98.935% with
  the late g-access mode. Every other indexed hash is unchanged.
- Catalogue: Aztec Challenge's frame hash moves (above) and is
  re-captured; the other twelve entries' hashes are unchanged.
- Every testbench program with a reference was run on the 6569, and those
  with an 8565 reference on the 8565: 41 frames changed, and every one that
  can be compared moved towards its reference. `fetchsplit` goes from 158 to
  0 (6569) and 289 to 154 (8565), `spritepriorities/test1` from 1,584 to 0,
  and the seven `videomode` programs from 31-129 to 0-79.
- Unit tests pin the dot-4 rule on the 6569 and the next-cell rule on the
  8565 at XSCROLL 0 and 5, the late falling BMM on both chips, the rising BMM
  on the 8565 and the 6569's ROM address mix. Each fails without its rule.

### What remains

#1661 resolved the first two items; see
[Opened border background](#opened-border-background-1661).

- `modesplit` (6569): 36 pixels at column 0's first dots, before the first
  load. The sequencer shows zero bits with zeroed matrix entries there; VICE
  keeps the previous line's last entries, so ECM selects `$D022`. Same cause
  as `border` (#1661).
- `vicii_timing`: `BC0`-`BC3` at the right edge (450 on the 6569) and the
  "open border with" rows at the right edge (81), the #1661 side-border
  class, plus the sprite X-position and X-expand rows (574).
- VICE itself differs from the `modesplit` references by one dot at some
  mode edges: 348 pixels on the 6569 and 124 on the 8565. Emu198x now
  follows VICE there.

## Opened border background (#1661)

### Finding

Bauer (section 3.7.3): outside the display column, and while the vertical
border flip-flop is set, "the last current background color is displayed".
VICE (`draw_graphics8`) keeps loading the shift register there, with zero
graphics and the matrix and colour entries of the last g-access in the
window; idle g-accesses leave those entries at zero. The renderer instead
filled the display column under the vertical border with `$D021`, and
showed zero entries beside the fetch window. An opened border therefore
showed `$D021` where the last character's background belongs: its ECM
background register in `vicii_timing`'s `BC0`-`BC3` rows ("continuation
of last character bg", per the program's source), and black in `border`'s
hires bitmap, whose idle lines leave the entries at zero.

The sequencer now draws every visible cell. Outside the window and under
the vertical border it receives zero graphics with the last matrix and
colour entries, which the engine keeps from the last g-access in the
window. Column 39's remaining pixels now spill into an opened side border
at XSCROLL above 0, as in VICE.

### Measurement

Disagreements with VICE x64sc 3.10's screenshot, before and after:

| Program | Chip | Before | After |
| --- | --- | ---: | ---: |
| `border` (`-250`) | both | 6,470 | 0 |
| `modesplit` | 6569 | 36 | 0 |
| `modesplit` | 8565 | 0 | 0 |
| `vicii_timing` (`-a5`) | both | 1,105 | 493 |
| `colorsplit` | both | 0 | 0 |

- Survey: `border` 93.806% to 100.000% and `vicii_timing` 98.935% to
  99.521%. Every other indexed hash is unchanged.
- Across every testbench program with a reference, 11 frames changed and
  every one moved towards its reference: `border-bm-idle` from 8,128
  disagreements to 42, `border-bm-ysh` from 5,422 to 32 and
  `border-bm-ysh2` from 4,742 to 32.
- Catalogue: all thirteen entries pass, ordinary and fresh-runtime replay,
  with the version-10 hashes.
- Unit tests pin the ECM background beyond the window and black under the
  vertical border in hires bitmap mode; both fail without the change.

### What remains

`vicii_timing`'s 493 disagreements with VICE are all on its sprite rows:
the mid-line sprite X-position and X-expand writes classified under
[Mid-line XSCROLL](#mid-line-xscroll-1620).

## Non-goals

This campaign does not expand indefinitely into every C64 peripheral or media
format. G71 support, a generic drive-trace refactor and future disk-geometry
abstraction remain separate work unless a selected closure case requires one.
User-port devices, network adapters and other unrelated expansion breadth do
not keep the campaign open.

The campaign also does not claim physical-hardware conformance from VICE,
reSID or Emu198x-produced output. Software comparison, shared-lineage
comparison and physical measurement remain distinct evidence classes.

## Pivot gate

The broad C64 push ends when:

- the declared PAL VIC-II strict cases pass and the three initial worst
  categories have either strict representative assertions or precise retained
  disagreement signatures;
- every remaining breadth-survey disagreement is fixed, scoped out, or
  recorded as blocked on stronger evidence;
- the SID oracle boundary and all accepted deviations are explicit;
- selected PAL, NTSC, breadbin and C64C catalogue entries pass ordinary and
  deterministic replay gates;
- the CPU and Lorenz corpus provenance is pinned and the chained-finalizer
  result is represented correctly; and
- a revision-keyed closure report records every gate and its evidence class.

After that gate, C64 work becomes failure-driven. New work must begin from a
real-software failure, a comparator disagreement, new primary or hardware
evidence, or an explicit expansion of the supported configuration claim.

## Progress log

| Date | Step | Result |
| --- | --- | --- |
| 2026-08-08 | Campaign baseline | Assessment and ordered closure work recorded at revision `bdb07858`. The PAL 6569 breadth survey ranges from 69.294 percent for `colorfetchbug` to 100 percent for `dmadelay`; the results are diagnostic fractions rather than conformance rates. |
| 2026-08-08 | 1. Live snapshot state | Commit `6a8cad9c` serialises the active VIC-II fetch/draw pipeline and queued SID output, bounds the diagnostic audio queues and replaces the incomplete serde-skip check with a recursive zero-skip audit. |
| 2026-08-08 | 1. Catalogue replay | Commit `bdb07858` adds fresh-runtime snapshot replay to every C64 catalogue entry. The complete 13-entry PAL matrix passes both ordinary and replay assertions across boot, disk, tape, cartridge and three drive families. |
| 2026-08-08 | 2. Revision-keyed VIC-II survey | The focused wrapper pins all 37 consumed PRG, PNG and ROM inputs for 17 programs across 13 categories, admits exact integer pixel counts from the Rust producer, and writes a path-free report under the full source revision. The upstream testbench revision and per-image evidence provenance remain explicit unresolved boundaries. |
| 2026-08-08 | 3. Late-badline display phase | Commit `74f31553` separates entering Phi1 display state from the Phi2 badline transition, consumes live pre-increment VMLI, and generates mode-correct idle output beneath an opened vertical border. Five survey cases improve and eight remain identical. All 13 catalogue frame/audio hashes recapture unchanged at routing version 3, then pass ordinary and fresh-runtime replay gates. The first-three invalid c-access contract remains open. |
| 2026-08-08 | 3. BA-to-AEC handover | Commit `9176e269` adds an explicit CPU-side Phi2 bus sample, derives AEC from consecutive aggregate BA-low cycles and stores `$FF` plus the supplied CPU nibble for the three invalid forced-badline c-accesses. All five registered `colorfetchbug` programs now match exactly. Snapshot envelope version 5 preserves a mid-handover state and runtime queries expose the bus and sequencer fields. All 13 catalogue entries retain their frame and audio hashes at `FRAME_ROUTING_VERSION` 4 and pass ordinary plus fresh-runtime replay verification. Normalised VICE and Emu198x traces rule out an upstream IRQ phase error at the critical `sequencer-bug` trigger, then expose a two-cycle late-window excess before the separate delayed C-data output question. |
| 2026-08-08 | 3. Far-edge late-badline window | Commit `d140a36f` gives the cycle-53 `$D011` transition one remaining c-access and keeps the exhausted window distinct from the ordinary schedule. `sequencer-bug` rises from 96,266 to 104,394 matching pixels; all 16 other indexed planes remain unchanged. Snapshot version 6 preserves pending, exhausted and source-resolved states. All 13 catalogue hashes remain unchanged at routing version 5 and every entry passes ordinary plus fresh-runtime replay verification. The residual is 54 pixels across eight rows and is now isolated to delayed C-data output sequencing. |
| 2026-08-13 | 3. Far-edge C-data and hidden-output counter state | Commit `70cd523b` keeps two resident output cells visually hidden; only the first following idle g-access suppresses VC/VMLI, while the active g-access behind the second advances them. A bounded 12-bit carry network replaces the fixture-specific displaced-slot repair. `sequencer-bug` rises from 104,394 to 104,418 matching pixels; the full survey confirms it is the only changed hash and all five `colorfetchbug` programs remain exact. The strict lane retains 30 disagreements: two colour-ring dots and a 28-pixel outline at the active-g-access/delayed-output boundary. The higher 104,446 two-suppression experiment is rejected because it contradicts Hoxs64's hidden counter state. Snapshot version 8 preserves the output delay and live carry, and frame-routing version 7 identifies the output contract. All 13 catalogue entries pass ordinary and fresh-runtime snapshot replay. The colour-resolution ring, output-stage split and separate post-badline `videomode` phase-accounting lead remain open. |
| 2026-10-06 | SID waveform generator (#777) | The pulse comparator drives high while `acc >= PW` (it was inverted), ring modulation substitutes `MSB EOR NOT source-MSB` and is blocked by sawtooth, the triangle's DAC bit 0 is grounded, TEST lets the noise register drift to all ones instead of reseeding it each cycle, write-only reads return the decaying data-bus value, and a deselected waveform leaves the DAC input floating and fading. VICE `ringmod`, `busvalue`, `osc3-wave0` and `bitfade` programs pass on the 6581 and 8580 models. Snapshot version 9 carries the new state; audio routing version 5 re-captures the eight music entries' audio hashes, with every frame hash and the five silent entries unchanged. All 13 entries pass ordinary and fresh-runtime snapshot replay. |
| 2026-10-06 | 3a. Raster-edge phase (#796) | Planned. A dot-0 colour rule matched `greydot` on both chips but left 51 wrong pixels in each `colorfetchbug` program. The cause: the CPU sees the line edge and raster IRQ 2 cycles early, and colour writes reach the screen 2 cycles late. A prototype that fixed both took `screenpos` and `videomode` to 100% and regressed `dmadelay`, `sequencer-bug` and `spritefetchbug`. Stages A-D are recorded above; A-C merge as one unit. |
| 2026-10-06 | SID combined waveforms and noise write-back (#769) | The noise waveform reads the die-photo shift-register taps (20, 18, 14, 11, 9, 5, 2, 0), the 8580 reads reSID's sampled 8580 combined-waveform tables instead of a bitwise AND (the 6581 tables were already reSID's samples, now checked entry by entry), noise+pulse pulls bits down per model, and noise combined with another waveform writes its zeros back into the shift register, locking it until TEST refills it. VICE `wb_testsuite` passes 100 of 110 (none before; VICE 3.10's reSID passes 91), `noise_writeback_test1` and the 6581 `wf12nsr` pass, and 8580 combined-waveform agreement with real-chip OSC3 readings rises from 23/128/169/127 to 215/251/182/242 of 255. Audio routing version 6 re-captures six music entries' audio hashes, five of them from the taps alone; every frame hash is unchanged and all 13 entries pass ordinary and fresh-runtime snapshot replay. |
| 2026-10-06 | SID waveform-generator pipelines (#1606) | The accumulator powers up at `0x555555` and the noise register at `0x7FFFFE`; hard sync spares a destination whose source is synced as its MSB rises; the 8580's OSC3 reads triangle and sawtooth a cycle late; the pulse compare reaches the output a cycle late; the noise register shifts two cycles after bit 19 rises, with no write-back during the latch phase; 6581 sawtooth combinations pull the accumulator MSB low. 8580 register writes stay undelayed, as `writedelay` measures. VICE `oscinit`, `detect`, `osc_topbit`, `noise_writeback_test2`, `writedelay` and `noiselfsrinit` pass; `wb_testsuite` and the 8580 `wf12nsr` keep their residuals. Real-chip OSC3 agreement for pulse rises to 256 of 256 on both models, 6581 pulse+saw from 136 to 256 and pulse+saw+triangle from 253 to 256. Snapshot version 13 carries the pipeline state. Audio routing version 7 re-captures all eight music entries' audio hashes and Aztec Challenge's frame hash, which follows the game's OSC3 random numbers; the other twelve frame hashes are unchanged and all 13 entries pass ordinary and fresh-runtime snapshot replay. |
| 2026-10-06 | 3a-A. CPU-visible raster edge | The raster counter that `$D011`, `$D012`, the raster compare, the badline comparator and the vertical border read now changes on cycle 1 (cycle 2 for line 0), as VICE's does. `greydot` stores at VICE's cycles and Lorenz's 14 runnable cases still pass. Alone, A regresses the `sequencer-bug` strict lane (92.235%), as planned; it merges with B and C. |
| 2026-10-06 | 3a-B. Write phases, sprite DMA, light pen | `$D011` and `$D017` write rules are expressed in the CPU write's own cycle: a far-edge `$D011` write in cycle 54 keeps one matrix access, and a `$D017` write in cycle 15 crunches (VICE `ChkSprCrunch`). Sprite BA follows the fetch chain's DMA bits, so sprites re-matched on lines 306-311 steal cycles as in VICE, which sets `sequencer-bug`'s main-loop phase. The light pen latches from CIA 1 port B bit 4 at VICE's X positions, which `spritefetchbug` uses to stabilise. `dmadelay` and `spritecrunch` reach 100%; `sequencer-bug`'s CPU now matches VICE store for store; its remaining 30 pixels are two colour splits drawn two cycles late, which stage C fixes. |
| 2026-10-06 | 3a-C. Colour and border stage | Colour registers and the side border are resolved two ticks after rendering, at VICE's phase, with the 6569 dot-0 rule; sprites sit under the border; zero graphics fill the side border. With A and B, every survey program reaches 100% except `border` (93.806%), `vicii_timing` (96.774%) and `spritefetchbug` (97.226%). `sequencer-bug` and `greydot` match exactly; `colorsplit` keeps only its XSCROLL rows. Frame-routing version 8 re-captures the C64 catalogue; snapshot version 11 carries the colour stage. |
| 2026-10-06 | 3a-D. 8565/8562 chip axis and grey dot | `VicModel` gains the HMOS-II 8565 and 8562, which the PAL and NTSC C64C now use. On them a colour-register write shows a light-grey dot where the 6569 keeps the old colour. `greydot` matches its 8565 reference exactly, and `colorsplit` keeps only its XSCROLL rows. Every breadbin lane and catalogue hash is unchanged. Snapshot version 12 carries the chip revision. On NTSC, `greydot`'s stores sit a cycle away from VICE's, which only the 8562's grey dots reveal. |
| 2026-10-07 | CIA serial port and user-port signals (#797) | The 6526's two-underflows-per-bit shift register becomes VICE's `sdr_delay` pipeline: CNT toggles about 1.5 cycles after each Timer A underflow, SP changes on falling CNT edges, a byte written mid-transfer chains on, and the SDR interrupt lands two cycles after the eighth bit. Input mode shifts SP in on rising CNT edges and loads the SDR after eight. Timer A and B count rising CNT edges (MiSTer's pipeline latency; VICE leaves it a TODO) and Timer B mode 11 gates on CNT. /PC strobes low for the cycle after a port B access. VICE `cia-sp-test` one-shot (old and new CIA) and all four `cia-icr-test` programs move from fail to pass; `cia-sp-test` continuous, `cia-icr-test2`, `cia-sdr-init`/`load`/`delay`, `ciavarious` 1–14 and Lorenz `cntdef`/`cnto2` pass. A user-port loopback (SP1/CNT1 to SP2/CNT2) carries every byte value from CIA1 to CIA2. Snapshot version 14. |
| 2026-10-07 | Mid-line XSCROLL (#1620) | The graphics sequencer loads each cell at the XSCROLL that stands after the CPU access of its g-access cycle, as VICE does, instead of one value latched per line; a write in cycle 55 misses column 39. `colorsplit` matches exactly on the 6569 and 8565 (952 and 960 disagreements before). `vicii_timing` rises from 96.774% to 98.468% and `spritefetchbug` from 97.226% to 98.211%; every other survey hash is unchanged. `border` does not share the cause; its residual and `vicii_timing`'s are classified under [Mid-line XSCROLL](#mid-line-xscroll-1620). Snapshot version 15 carries the sequencer's last load. Frame-routing version 9; all 13 catalogue entries pass ordinary and fresh-runtime replay with unchanged hashes. |
| 2026-10-07 | Mode bits at the shifter (#1660) | The graphics sequencer keeps raw bits and decodes each dot with the mode as it stands, as VICE does: MCM from dot 4, ECM and BMM rising at dot 4 and falling at dot 6 on the 6569, after dot 7 on the 8565. It draws each cell as it leaves the colour stage. The g-access address reads `$D011` a cycle late on the 8565, and a falling BMM a cycle late on the 6569. `modesplit` goes from 5,990 to 36 disagreements with VICE on the 6569 and from 5,800 to 0 on the 8565; `vicii_timing` rises from 98.468% to 98.935%, and every other survey hash is unchanged. A multicolour "01" pair now counts as background for sprite priority, which re-captures Aztec Challenge's frame hash; the other twelve catalogue entries are unchanged. Snapshot version 16 carries the sequencer. Frame-routing version 10. |
| 2026-10-07 | Opened border background (#1661) | Outside the display window and under the vertical border the sequencer keeps loading zero graphics with the last g-access's matrix and colour entries, as VICE does, so an opened border shows the last character's background instead of `$D021`. `border` reaches 100% (6,470 disagreements before), `modesplit` matches VICE on both chips and `vicii_timing` rises from 98.935% to 99.521%; every other survey hash is unchanged. Snapshot version 17 carries the entries. Frame-routing version 11; all 13 catalogue entries pass ordinary and fresh-runtime replay with unchanged hashes. |

## Related Documents

- [C64 architecture review](c64-architecture-review.md)
- [PAL 6569 late-badline display phase](c64-late-badline-display-phase.md)
- [C64 BA-to-AEC handover](c64-ba-aec-handover.md)
- [PAL 6569 far-edge late-badline DMA window](c64-far-edge-badline-window.md)
- [PAL 6569 far-edge forced-badline C-data](c64-forced-badline-cdata-pipeline.md)
- [October catalogue](october-catalogue.md)
- [Save state format](save-state-format.md)
- [Live-machine serde](savestate-live-machine-serde.md)
- [MOS 6502](../chips/mos-6502.md)
- [MOS 6526 CIA](../chips/mos-cia-6526.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [MOS 6581 / 8580 SID](../chips/mos-sid-6581.md)
- [Commodore 64 system overview](../systems/commodore-c64.md)
- [Golden-image capture](../processes/golden-image-capture.md)
- [C64 VIC-II reference survey](../processes/c64-vicii-vice-survey.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [C64 catalogue manifest](../../crates/emu198x-catalogue/manifest/c64.toml)
