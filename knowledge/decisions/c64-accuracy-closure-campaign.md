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
| `vicii_timing` | 84.720% |
| `screenpos` | 87.800% |
| `videomode` | 88.980% |
| `border` | 92.533% |
| `spritecrunch` | 95.190% |
| `spritefetchbug` | 97.004% |
| `sb_sprite_fetch` | 98.578% |
| `gfxfetch` | 99.325% |
| `sequencer-bug` | 99.971% |
| `greydot` | 99.993% |
| `spritedma` | 99.998% |
| `dmadelay` | 100.000% |
| `colorfetchbug` | 100.000% for each of five programs |

The complete frame-routing-version-7 survey confirms these results. Relative
to revision `d140a36f`, only the `sequencer-bug` indexed plane changes; every
other registered score and hash is identical.

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
percent. A separate strict lane now requires pixel and indexed-hash identity
for all five PAL 6569 colour-fetch-bug programs; all five pass at revision
`d140a36f` and remain exact after the far-edge C-data correction. The
`sequencer-bug` lane retains its exact 30-pixel disagreement signature rather
than a rounded threshold. There is no strict 6567R56A or 8565 comparison yet.
The PAL 6569 `greydot` reference does not establish 8565 grey-dot behaviour.

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
  profiles. C64C selects the 8580 and 6526A, but its video profile still uses
  the 6569 or 6567 implementation rather than a distinct 8565 model.

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
belong to the unimplemented PAL 6569 colour-resolution ring. The outline is
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

- CIA timer and interrupt behaviour is well exercised, but external CNT, SP
  and CIA2 FLAG sources remain approximate or unattached.
- SID open-bus decay, TEST ramp details, ring-modulation polarity and 8580
  combined-waveform/noise interactions need stronger coverage.
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
   visual output required by the 28-pixel `sequencer-bug` outline, then model
   the PAL 6569 colour-resolution ring needed by its two dot-zero pixels.
   Classify the post-badline phase-accounting lead in `videomode`, then address
   `vicii_timing`. Introduce explicit dot or
   Phi1/Phi2 stages where the evidence requires them. A change must improve
   the targeted oracle without absorbing an unexplained regression in a
   stronger lane. Treat the testbench program and reference image as the
   black-box contract, inspect vendored VICE 3.10 as implementation evidence,
   and use Hoxs64, VirtualC64 or MiSTer where they can independently classify a
   residual. VICE source is not itself the specification.
4. Promote corrected representative cases to strict assertions and broaden
   within each category before making a category-level claim. Add strict
   6567R56A and 8565 contracts only after suitable model-specific references
   are registered.
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
