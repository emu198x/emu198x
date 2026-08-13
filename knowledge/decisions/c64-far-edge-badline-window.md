# Decision: Constrain the PAL 6569 far-edge late-badline DMA window

**Date:** 2026-08-08
**Status:** BINDING
**Implementation revision:** `d140a36f`
**Follow-up qualification:** `70cd523b`

## The question

How many matrix c-accesses remain when a `$D011` write creates a PAL 6569
badline at the far edge of the ordinary fetch window?

## Evidence

The selected `sequencer-bug` program writes `$3B` to `$D011` at VICE monitor
cycle 53. VICE 3.10 records the shortened matrix-DMA length as
`54 - cycle`, leaving one c-access after that write. The following instruction
then reaches its monitored store at cycle 55. This is distinct from the
ordinary cycle-15-through-54 matrix-fetch schedule.

Emu198x ticks the VIC-II before completing the CPU access for that machine
cycle. The `$D011` write therefore reaches `Vic::write` with engine cycle 53
recorded, and the entering engine-cycle-53 tick is the one remaining access
opportunity. Before revision `d140a36f`, the generic through-cycle-54
predicate reopened a second c-access on the following tick. It also kept the
CPU at the following opcode for two cycles beyond the instruction schedule
observed in VICE.

The source-resolved trace separates this excess from sprite DMA. On the
critical line it observes:

| Entering Emu198x phase | Badline BA source | Sprite BA source | c-access |
| --- | --- | --- | --- |
| 53 | low | high | attempted |
| 54 | high | high | not attempted |
| 55 | high | low | not attempted |

Here “low” means that the named source contributes to aggregate BA. The phase
labels describe Emu198x's entering-cycle convention. They must not be read as
a new claim about an otherwise unobserved physical BA transition between
differently labelled VICE monitor phases.

A broader experiment applied a shortened counter to every dynamically forced
late badline. It fixed the far-edge instruction schedule but removed the
rightmost eight pixels from all five exact `colorfetchbug` references. The
existing earlier-forced schedule is therefore retained. The selected output
evidence supports a narrow far-edge correction, not a general rewrite of all
late-badline lengths.

## The decision

A `$D011` write records the VIC-II engine cycle at which the CPU access
completed. On the following VIC-II tick, a false-to-true badline transition at
recorded cycle 53 or later creates an explicit far-edge window with:

```text
remaining c-accesses = max(54 - recorded cycle, 0)
```

Consequently:

- a recorded cycle-53 write has one remaining c-access;
- a recorded cycle-54-or-later write has none; and
- an ordinary badline or an earlier forced badline continues to use the
  established schedule evidenced by the exact colour-fetch lane.

The explicit state is optional. `None` selects the established schedule.
`Some(n)` identifies a far-edge window, and `Some(0)` remains present after
that window is exhausted. The exhausted value must not collapse to `None`,
because doing so would let the generic cycle-54 predicate reopen the access.
The marker is cleared at raster-line wrap.

Badline BA, sprite BA and c-access activity are latched separately for the
most recently completed VIC-II phase. Aggregate BA remains the logical OR of
the two BA sources. This changes neither the aggregate BA-to-AEC handover nor
the invalid matrix-data contract.

The correction does not shift global CPU timing, change the machine's
VIC-before-CPU scheduling order, delay badline activation, or change ordinary
badline DMA. Those alternatives affect already exact output and solve the
wrong boundary.

## Persistence and inspection

The recorded `$D011` phase, remaining-window marker and source-resolved bus
latches affect future execution or expose the current arbitrary-cycle phase.
They are serialised with the VIC-II. C64 snapshot envelope version 6 preserves
both a pending cycle-53 write and an exhausted `Some(0)` marker. A restored
exhausted marker must keep the next cycle free of a reopened c-access.

The runtime query surface exposes:

- `vic.badline_ba_low`;
- `vic.sprite_ba_low`;
- `vic.c_access_active`;
- `vic.pending_d011_write_cycle`;
- `vic.late_badline_window`; and
- `vic.late_badline_fetches_remaining`.

The remaining-count query returns `null` for the established schedule, a
positive integer for a live far-edge window and zero for an exhausted one.
`vic.late_badline_window` identifies the explicit window in both its live and
exhausted states; it is not an “active DMA” predicate.

Because the corrected window changes rendered output,
`FRAME_ROUTING_VERSION` is 5.

## Verification

Revision `d140a36f` adds directed checks for:

- exactly one attempted c-access after a recorded cycle-53 write;
- no attempted c-access after a recorded cycle-54 write;
- preservation of the established schedule for an earlier forced badline;
- preservation of `Some(0)` until line wrap;
- source-resolved badline BA, sprite BA and c-access phases in the
  `sequencer-bug` trace; and
- snapshot round trips before the remaining access and after the window is
  exhausted.

All 94 `mos-vic-ii` unit tests and all 12 chip oracle tests pass. The complete
runtime and catalogue test suites, strict Clippy checks and both focused
external lanes also pass.

All five registered PAL 6569 `colorfetchbug` programs remain exact at
104,448 of 104,448 classified pixels with byte-identical indexed-plane
hashes. `sequencer-bug` improves from 96,266 to 104,394 matching pixels, a net
gain of 8,128 and a final match fraction of 99.948 percent. Sixteen of the
seventeen registered indexed planes are byte-for-byte unchanged from the
preceding survey.

The clean report is retained at
`target/accuracy/c64-vicii-survey/d140a36f782862706e04b15272bf5f7f4a145862/report.json`.
The remaining 54 differing pixels occupy reference rows 34, 35 and 37–42;
all other rows match exactly.

All 13 C64 catalogue entries retain their existing frame and audio hashes at
frame-routing version 5. Each produces both `PASS` and `SNAP-PASS` with
snapshot envelope version 6. This is regression and determinism evidence, not
an independent hardware oracle.

## Downstream C-data qualification

The 2026-08-13 follow-up preserves this one-access window unchanged and models
what survives after it. Two already-resident C/V/G cells remain visually
hidden after the Phi2 transition. Only the first following g-access remains
idle and suppresses VC/VMLI; the active g-access behind the second hidden cell
advances both counters. The sole invalid c-access starts a bounded 12-bit
C-data carry for eligible output on the next RC-zero display line.

The full survey changes only the `sequencer-bug` indexed plane relative to
revision `d140a36f`, and all five colour-fetch cases remain exact. The literal
model reaches 104,418 of 104,448 matching pixels. Its 30 disagreements are two
colour-ring dots and a 28-pixel character outline at the remaining
active-g-access/delayed-output boundary. Snapshot envelope version 8 preserves
the output delay and carry, while frame-routing version 7 identifies the
revised output contract.

This does not revise the number of c-access attempts specified here. The
downstream state is specified separately by
[PAL 6569 far-edge forced-badline C-data](c64-forced-badline-cdata-pipeline.md).

## Evidence boundary

This decision governs only the number of matrix c-access attempts remaining
after the selected far-edge badline transition. The historical 54-pixel
`sequencer-bug` signature motivated, but is not specified by, this decision.
The later output, counter and C-data state is governed by the dedicated
follow-up decision.

The decision is evidenced for the PAL 6569 profile and the staged
`sequencer-bug` and `colorfetchbug` programs. It does not establish equivalent
behaviour for 6567R8, 6567R56A or 8565. The comparison uses software-produced
reference images and VICE instruction/source evidence; it is not a physical
hardware capture or a gate-level silicon explanation.

## Drift triggers

Reject changes that:

- allow a cycle-53 far-edge write to perform a second generic c-access;
- collapse an exhausted `Some(0)` window into the ordinary `None` schedule;
- apply the far-edge truncation to earlier forced badlines without preserving
  all five exact colour-fetch references;
- shift global CPU or VIC-II timing to repair this local window;
- merge badline and sprite BA causes so the trace can no longer distinguish
  them;
- omit the pending or exhausted state from snapshots; or
- attribute closure of the historical 54-pixel signature to this access-count
  decision rather than the downstream C-data decision.

Any change to `$D011` scheduling, badline-window selection, c-access
predicates, BA-source aggregation or snapshot state requires the directed
tests, strict colour-fetch lane, sequencer trace and clean survey to be rerun.

## Related Documents

- [PAL 6569 late-badline display phase](c64-late-badline-display-phase.md)
- [C64 BA-to-AEC handover](c64-ba-aec-handover.md)
- [PAL 6569 far-edge forced-badline C-data](c64-forced-badline-cdata-pipeline.md)
- [C64 accuracy closure campaign](c64-accuracy-closure-campaign.md)
- [C64 architecture review](c64-architecture-review.md)
- [Save state format](save-state-format.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [C64 VIC-II reference survey](../processes/c64-vicii-vice-survey.md)
- [VIC-II survey fixture notes](../../test-data/commodore/c64/vicii-vice-survey/README.md)
