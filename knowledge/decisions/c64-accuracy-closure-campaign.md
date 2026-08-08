# Decision: C64 accuracy closure campaign

**Date:** 2026-08-08
**Status:** ACTIVE
**Assessment revision:** `bdb07858`

## The question

What accuracy work must Emu198x complete before the active Commodore 64 effort
pivots from broad improvement to failure-driven maintenance?

## Current assessment

The strongest supported slice is the PAL breadbin C64 running ordinary disk,
tape and cartridge software. CPU functional coverage, deterministic state and
inspection are strong. The current VIC-II comparison nevertheless contains
large, repeatable differences in late-badline, sprite-sideband and
register-timing cases. Parity with mature reference emulators is therefore not
yet defensible across the behaviour Emu198x claims to support.

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

The PAL 6569 breadth survey runs one canonical program per category from the
VICE VIC-II testbench and compares its 384 x 272 reference image after the
fixed 16-pixel crop. Pixels are classified by nearest C64 palette index, so the
measurement tests digital colour-index output rather than analogue colour
reproduction.

| Category | Matching pixels |
| --- | ---: |
| `colorfetchbug` | 69.294% |
| `sb_sprite_fetch` | 76.519% |
| `vicii_timing` | 84.720% |
| `screenpos` | 87.800% |
| `videomode` | 88.980% |
| `sequencer-bug` | 92.128% |
| `border` | 92.390% |
| `spritecrunch` | 95.190% |
| `spritefetchbug` | 96.526% |
| `gfxfetch` | 99.325% |
| `greydot` | 99.993% |
| `spritedma` | 99.998% |
| `dmadelay` | 100.000% |

These are pixel-match fractions, not test pass rates. Each row is one
representative program. The third-party testbench reference set has uneven
per-image provenance: some images are constructed expectations, some cases
describe measured C64 behaviour, and the set is not uniformly a direct
hardware capture or the output of a second emulator family. The staged corpus
upstream revision is unresolved; the survey runner pins the 29 selected input
files by byte identity. The survey establishes where to investigate; it does
not turn partial matches into conformance claims.

The strict lanes currently require at least 99 percent for PAL 6569
`gfxfetch`, at least 99.9 percent for PAL 6569 `spritedma`, and at least 94
percent overall for NTSC 6567R8 `gfxfetch`. The NTSC residual is concentrated
in the viewport-wrapping rows; overlapping content is approximately 99.3
percent. There is no strict 6567R56A or 8565 comparison yet. The PAL 6569
`greydot` reference does not establish 8565 grey-dot behaviour.

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

- Snapshot envelope version 4 preserves the active VIC-II sprite, fetch and
  render pipeline plus queued mixed and per-voice SID audio. Active-sprite and
  non-empty-audio regressions compare restored execution with an unforked
  machine. The recursive C64 serde audit currently finds no skipped state in
  the CPU, VIC-II, SID, CIA, board, IEC, drive or runtime stack. This state was
  established by commit `6a8cad9c`.
- At commit `bdb07858`, all 13 C64 catalogue entries produce `PASS` and
  `SNAP-PASS`. The matrix covers firmware-only boot, D64, D81, G64, TAP,
  EasyFlash, Final Cartridge III, Action Replay, 1541, 1571 and 1581 paths.
  Its frame and audio hashes are Emu198x regression oracles, not independent
  hardware evidence. Every entry currently uses a PAL profile.
- The runtime has PAL and NTSC breadbin profiles plus PAL and NTSC C64C
  profiles. C64C selects the 8580 and 6526A, but its video profile still uses
  the 6569 or 6567 implementation rather than a distinct 8565 model.

## Why a parity claim is not yet defensible

### Working diagnosis: late-badline display indexing

The first VIC-II disagreement is also a structural warning. The VICE
`colorfetchbug` notes distinguish the display-matrix-line index from the
video-matrix-line index when a badline begins after cycle 14. The current
renderer selects `screen_row` and `colour_row` from the geometry display cycle
and has no separate draw-side matrix index. This is a design hypothesis until
a focused change proves that it explains the registered pixels, but it gives
the worst survey result a concrete implementation seam.

The open question is whether adding that state is sufficient to close the
reference image without disturbing ordinary badlines. A strict before/after
comparison, rather than the resemblance of the implementation to another
emulator, answers that question.

### Other claim boundaries

The VIC-II currently advances one Phi2 cycle and renders its eight pixels as a
batch. CPU register writes become visible after that batch. The model therefore
has no explicit dot or half-cycle write contract for cases that change display
state during those eight pixels. The `sb_sprite_fetch` and `vicii_timing`
residuals make that boundary material rather than theoretical.

Other claim boundaries remain:

- CIA timer and interrupt behaviour is well exercised, but external CNT, SP
  and CIA2 FLAG sources remain approximate or unattached.
- SID open-bus decay, TEST ramp details, ring-modulation polarity and 8580
  combined-waveform/noise interactions need stronger coverage.
- Ultimax unmapped reads do not yet model the required open-bus behaviour.
- REU transfers complete as one machine operation rather than participating in
  cycle-visible bus arbitration.
- The catalogue has no NTSC or C64C entry.
- VICE differential results are console measurements rather than a retained,
  revision-keyed report package. The VICE 3.10 source holding is identified by
  release, but its exact upstream source revision has not been recovered.
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
2. Make the VIC-II comparison reproducible as a revision-keyed report, with
   fixture identity, model, crop, palette-classification method and per-case
   result retained. Keep the current survey as the starting measurement.
3. Close the worst shared VIC-II seams in measured order:
   `colorfetchbug`, `sb_sprite_fetch`, then `vicii_timing`. Introduce explicit
   dot or Phi1/Phi2 stages where the evidence requires them. A change must
   improve the targeted oracle without absorbing an unexplained regression in
   a stronger lane. Treat the testbench program and reference image as the
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
| 2026-08-08 | 2. Revision-keyed VIC-II survey | The focused wrapper pins all 29 consumed PRG, PNG and ROM inputs, admits exact integer pixel counts from the Rust producer, and writes a path-free report under the full source revision. The upstream testbench revision and per-image evidence provenance remain explicit unresolved boundaries. |

## Related Documents

- [C64 architecture review](c64-architecture-review.md)
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
