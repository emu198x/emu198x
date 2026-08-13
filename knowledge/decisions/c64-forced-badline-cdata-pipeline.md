# Decision: Preserve PAL 6569 far-edge forced-badline C-data state

**Date:** 2026-08-13
**Status:** BINDING
**Implementation revision:** `70cd523b`

## The question

What output, counter and C-data state survives when a `$D011` write creates a
PAL 6569 badline at the far edge of the matrix-fetch window?

## Evidence

The existing far-edge decision establishes that the selected cycle-53 write
leaves one attempted c-access. It does not establish what the display pipeline
does with that access after the fetch window has closed.

At revision `d140a36f`, Emu198x matched 104,394 of the 104,448 classified
pixels in the staged PAL 6569 `sequencer-bug` reference. The 54 disagreements
were confined to eight rows. CPU and VIC-II traces had already aligned the
critical write and following instruction schedule with VICE, and all five
registered `colorfetchbug` programs were exact. Changing global CPU cadence,
BA-to-AEC ownership or the number of far-edge c-accesses would therefore have
reopened stronger evidence.

Hoxs64 represents this case with two distinct mechanisms:

- two C/V/G cells already resident in the output path remain visually hidden
  from the newly active display state, while the second is nevertheless backed
  by an active g-access; and
- selected bits from packed 12-bit C-data entries propagate through a bounded
  carry network on the following eligible RC-zero display line.

Applying that model to Emu198x closes 24 of the 54 indexed disagreements in
`sequencer-bug` while leaving all five colour-fetch references exact. The
result supports a local C-data and counter-state correction. It does not
support changing the board scheduler or introducing a C64-wide extra tick.

An experiment that suppressed VC/VMLI for both visually hidden cells reached
104,446 matching pixels. It was rejected because Hoxs64 advances the active
g-access behind the second cell. Preserving a higher fixture score by
discarding that hidden state would contradict the implementation evidence and
turn the remaining direct-renderer compression into a compensating error.

## The decision

One C64 machine tick remains one Phi2 cycle. The VIC-II, CIAs, interrupt and
RDY wiring, CPU bus transaction and SID retain their established order within
that tick. The investigation found no C64 double-tick or overtick analogous to
the former Z80-family cadence defect.

When a post-VIC far-edge `$D011` transition creates a badline, two C/V/G cells
already in the output path retain idle-looking output before the newly active
display path becomes visible. The first following g-access is still idle and
does not advance VC or VMLI. The second visually hidden cell is backed by an
active g-access and advances both counters. Visual output delay and counter
advance are therefore related pipeline stages, not one shared predicate.

The sole invalid far-edge c-access arms a bounded C-data carry state. Each
matrix entry is treated as a 12-bit value containing the screen byte and
colour nibble. On an eligible non-bad RC-zero display line, the live VMLI
entry and the age-selected entry are merged using the Hoxs64 carry network;
the selected bits become the carry input to the following eligible merge.
The network expires after 40 VIC-II clocks.

A new qualifying far-edge badline always starts a new carry origin. It does
not retain the age or carry value from an earlier origin.

This state is specific to the far-edge forced-badline path. Ordinary badlines
and earlier forced badlines retain the schedules established by the exact
colour-fetch lane.

## Persistence and inspection

The two-cell output delay, carry age and 12-bit carry value affect future
output and are serialised with the VIC-II. C64 snapshot envelope version 8
preserves them across an arbitrary-cycle restore.

The runtime query surface exposes:

- `vic.forced_badline_output_delay`;
- `vic.forced_badline_cdata_carry_pending`;
- `vic.forced_badline_cdata_carry_age`;
- `vic.forced_badline_cdata_carry_value`;
- `vic.forced_badline_cdata_destination_vmli`; and
- `vic.forced_badline_cdata_eligibility_cycles_remaining`.

The destination query reports live VMLI rather than a stored matrix slot. The
eligibility query reports the remaining part of the bounded 40-clock window;
neither query claims that a merge will occur unless the RC, display and
badline conditions are also satisfied.

Because the correction changes observable pixels, `FRAME_ROUTING_VERSION` is
7.

## Verification

Directed chip tests establish that:

- both resident output cells retain idle-looking output;
- the first following g-access does not advance VC or VMLI;
- the active g-access behind the second hidden output cell does advance them;
- the far-edge invalid c-access starts a zero-aged, zero-valued carry origin;
- a later qualifying origin replaces an earlier live origin;
- the packed screen-byte and colour-nibble entries follow the bounded 12-bit
  merge network; and
- the carry expires outside its 40-clock window.

The strict PAL 6569 output lanes establish that each of the five registered
`colorfetchbug` programs remains exact at 104,448 of 104,448 classified
pixels. `sequencer-bug` improves from 104,394 to 104,418 matching pixels out of
104,448, or 99.971 percent. Its strict test asserts all 30 remaining
disagreements by coordinate and colour index.

The retained signature has two parts:

| Residual | Coordinates | Emu198x index | Reference index | Pixels |
| --- | --- | ---: | ---: | ---: |
| Dot-zero colour transition | `(32, 34)` | 11 | 12 | 1 |
| Dot-zero colour transition | `(64, 34)` | 12 | 11 | 1 |
| Character-outline foreground | `x = 32..39`, `y = 36..43`, outline only | 6 | 15 | 28 |

The two isolated dots require the PAL 6569 colour-resolution ring. The 8 x 8
outline exposes the unresolved separation between active g-access/counter
state and delayed visual output in the compressed direct renderer.

The complete post-correction 17-plane rerun confirms that `sequencer-bug` is
the only changed indexed plane. The other scores and hashes remain:

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
| `greydot` | 99.993% |
| `spritedma` | 99.998% |
| `dmadelay` | 100.000% |
| `colorfetchbug` | 100.000% for each of five programs |

These are digital palette-index comparisons against staged software-produced
references. They are not physical-silicon measurements.

## Evidence boundary

This decision specifies the state needed to reproduce the selected PAL 6569
far-edge output. Hoxs64 is independent implementation evidence for the shape
of the carry network; it is not a substitute for a die-level or
physical-hardware explanation.

The implementation still resolves a cell's eight colour indices as one
cycle-sized batch. It does not model the PAL 6569 colour-resolution ring that
can delay a register-backed colour at an individual dot. The two isolated
`sequencer-bug` pixels are classified against that separate colour-output
question. A colour-specific workaround for `$D020`, or any other individual
register, would fit this fixture without establishing the general hardware
rule and is therefore rejected.

The 28-pixel outline is not a C-data carry disagreement. It reflects the
direct renderer's remaining inability to retain delayed visual output while
advancing the active g-access and counters behind it. That output-stage split
must be modelled explicitly rather than suppressing the valid second counter
advance.

The `videomode` residual remains an independent post-badline phase-accounting
lead. It is not evidence for changing the C-data carry or the specified
counter phase behind the hidden cells.

The evidence is for the PAL 6569 profile. It does not establish the same
pipeline for 6567R8, 6567R56A or 8565, and it does not define analogue colour
output.

## Drift triggers

Reject changes that:

- advance VC or VMLI during the first following idle g-access;
- suppress VC or VMLI for the active g-access behind the second visually
  hidden output cell;
- make a qualifying far-edge origin inherit an earlier carry;
- let the carry survive beyond its bounded 40-clock eligibility window;
- apply this far-edge state to ordinary or earlier forced badlines without
  preserving all five exact colour-fetch cases;
- change machine cadence, CPU-write phase or BA-to-AEC ownership to repair the
  retained colour-output signature;
- introduce a register-specific colour-delay rule without a general PAL 6569
  colour-ring contract; or
- extend the PAL 6569 result to another VIC-II model without model-specific
  evidence.

Any change to the forced output delay, VC/VMLI advance, C-data packing or
carry merge requires the directed state tests, strict colour-fetch lane,
exact retained-disagreement assertion and full revision-keyed survey.

## Related Documents

- [PAL 6569 far-edge late-badline DMA window](c64-far-edge-badline-window.md)
- [PAL 6569 late-badline display phase](c64-late-badline-display-phase.md)
- [C64 BA-to-AEC handover](c64-ba-aec-handover.md)
- [C64 accuracy closure campaign](c64-accuracy-closure-campaign.md)
- [C64 architecture review](c64-architecture-review.md)
- [Save state format](save-state-format.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [C64 VIC-II reference survey](../processes/c64-vicii-vice-survey.md)
- [VIC-II survey fixture notes](../../test-data/commodore/c64/vicii-vice-survey/README.md)
