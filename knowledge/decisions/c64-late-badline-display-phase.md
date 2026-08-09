# Decision: Separate PAL 6569 Phi1 display state from late-badline Phi2 state

**Date:** 2026-08-08
**Status:** BINDING
**Implementation revision:** `74f31553`
**Follow-up qualification:** `d140a36f`

## The question

When a `$D011` write creates a PAL 6569 badline after cycle 14, which display
state governs that cycle's graphics access and rendering, and which state
governs the subsequent matrix access?

## Evidence

The staged VICE `colorfetchbug` notes state that a YSCROLL write during cycle
15 makes badline state active in cycle 16, while display logic does not
recognise the transition until cycle 17. VC, VMLI and the draw-side matrix
index advance only in non-idle display state. The resulting late badline
therefore contains 39 paired cells rather than the ordinary 40.

The VICE 3.10 cycle-exact implementation preserves that ordering explicitly.
`viciisc/vicii-cycle.c` performs the Phi1 graphics fetch and draw before its
Phi2 badline check and matrix fetch. `viciisc/vicii-fetch.c` increments VC and
VMLI only after a non-idle graphics fetch; an idle graphics fetch reads
`$3FFF`, or `$39FF` when ECM is set, without advancing them.
`viciisc/vicii-draw-cycle.c` advances its draw-side index only outside idle
state and supplies zero video-matrix and colour values while idle.

VirtualC64 independently separates the same concerns. Its `gAccess`
increments VC and VMLI only in display state and performs the `$3FFF` or
`$39FF` access while idle. Its matrix-access path remains separate from the
graphics display state.

Hoxs64 models a forced badline with an idle-delay state. Its cycle-16 graphics
access still observes idle, clears the delay and does not advance VC or VMLI;
the matrix access follows separately. Cycle 17 performs the first active
graphics access. This agrees with the staged test description and VICE
ordering without making any one emulator the specification.

The staged border tests distinguish the border flip-flops from the underlying
graphics pipeline. When software opens the vertical border, idle graphics or
the live display pipeline can become visible. The opened area must not retain
pixels left in the framebuffer by a preceding frame.

## The decision

At the start of every VIC-II cycle, Emu198x samples whether the chip entered
Phi1 in display or idle state. That sampled state governs:

- the graphics value rendered for the cycle;
- the matrix-line entry selected for display; and
- whether VC and VMLI advance after the graphics access.

Badline evaluation then occurs in the Phi2 part of the cycle. A newly detected
badline may clear idle state and enable that cycle's matrix access, but it must
not retroactively change the Phi1 graphics access or counter advance that has
already occurred.

For a badline forced after cycle 14, the relevant PAL sequence is:

| Cycle | Phi1 graphics and display phase | Phi2 badline and matrix phase |
| --- | --- | --- |
| 16 | Entering state is idle. Perform idle graphics behaviour and do not advance VC or VMLI. | Badline becomes active, idle state clears and matrix slot 0 is selected at VCBASE. |
| 17 | Display matrix slot 0, then advance VC and VMLI to slot 1. | Select matrix slot 1. |
| 18–54 | Display the current slot, then advance once. | Select the newly advanced slot. |
| 55 | Display slot 38, then advance to 39. | No matrix access. |

This gives 39 paired slots, numbered 0 through 38. An ordinary badline already
enters cycle 16 in display state and retains its existing 40-cell sequence.

Rendering uses the same pre-increment VMLI that the graphics phase consumes.
It no longer derives the displayed matrix column solely from raster geometry.

Within the horizontal display-cycle range, an opened vertical border reveals
fresh output from the current display pipeline. In idle state the graphics
access reads `$3FFF`, or `$39FF` when ECM is set, with zero video-matrix and
colour inputs and the appropriate mode-specific decoding. In display state it
reveals the live VMLI-selected cell. Closed borders continue to overlay the
border colour.

This decision does not define horizontal side-border output after the normal
display-cycle range. Its continuing shifter and idle behaviour remains a
separate pipeline question.

## Persistence and inspection

No new delayed machine state is required. The entering Phi1 state is a
per-tick sample derived from the serialised `idle_state`. Badline state, idle
state, border flip-flops, VC, VCBASE, RC, VMLI and the matrix buffers remain
part of `Vic` state, as does the live framebuffer.

Generating fresh active or idle output beneath an opened border removes a
dependence on pixels retained from the preceding frame. This is required even
though the framebuffer is serialised: snapshot fidelity must preserve current
output state without making future pixels depend on unrelated frame history.

VC, VCBASE, RC and VMLI have focused chip-level getters and direct tests.
Revision `9176e269` subsequently exposed `vic.badline`, `vic.idle_state`,
`vic.ba_low`, `vic.ba_low_cycles`, `vic.aec_low`, `vic.cpu_stalled`,
`vic.last_bus_data`, `vic.rc`, `vic.vc`, `vic.vcbase` and `vic.vmli` through
the runtime C64 query surface. `cpu.addr`, `cpu.data` and the added
`cpu.data_in` path make the CPU side inspectable at the same boundary. The
border flip-flops remain internal, so this decision does not claim that the
live debugger surface is complete.

The observable frame contract changes, so `FRAME_ROUTING_VERSION` is 3. All
13 C64 catalogue entries were recaptured; their frame and audio hashes were
unchanged. The version-3 manifest then passed ordinary and fresh-runtime
snapshot verification for all 13 entries.

The subsequent BA-to-AEC handover correction advances the engine contract to
`FRAME_ROUTING_VERSION` 4 and the C64 snapshot envelope to version 5. Its
13 catalogue entries retained their frame and audio hashes after recapture,
then passed ordinary and fresh-runtime snapshot verification at version 4.

Revision `d140a36f` adds source-resolved badline BA, sprite BA and c-access
queries together with the pending `$D011` phase and explicit far-edge-window
state. Snapshot envelope version 6 preserves those states. The far-edge
correction advances `FRAME_ROUTING_VERSION` to 5; all 13 catalogue entries
again retain their hashes and pass ordinary plus fresh-runtime replay gates.

## Verification

Focused tests establish that:

- a forced late badline leaves VC and VMLI at zero during cycle 16 and writes
  the first matrix slot there;
- cycle 17 consumes slot zero, advances once and selects slot one for the
  following matrix access;
- an ordinary badline retains its established slot-zero and slot-one sequence;
- an opened vertical border replaces stale framebuffer pixels with freshly
  generated idle output;
- an opened vertical border exposes a live display cell when display state is
  active;
- idle hires-bitmap output uses zero matrix colours; and
- idle multicolour-bitmap decoding retains D021 for zero pairs and black for
  the zeroed matrix-colour selections.

At implementation revision `74f31553`, all 91 `mos-vic-ii` unit tests and all
11 integration and oracle tests pass. The `runtime-commodore-c64` default test
suite and strict Clippy checks also pass.

The clean revision-keyed PAL 6569 survey changed five of its 13 indexed output
planes relative to the clean `100c613d` baseline:

| Survey case | Baseline | Revision `74f31553` | Exact change |
| --- | ---: | ---: | ---: |
| `colorfetchbug` | 72,376 / 104,448 (69.294%) | 96,568 / 104,448 (92.456%) | +24,192 |
| `sb_sprite_fetch` | 79,923 / 104,448 (76.519%) | 102,963 / 104,448 (98.578%) | +23,040 |
| `spritefetchbug` | 100,819 / 104,448 (96.526%) | 101,319 / 104,448 (97.004%) | +500 |
| `border` | 96,500 / 104,448 (92.390%) | 96,649 / 104,448 (92.533%) | +149 |
| `sequencer-bug` | 96,226 / 104,448 (92.128%) | 96,338 / 104,448 (92.235%) | +112 |

The indexed-plane hashes and exact counts for `gfxfetch`, `dmadelay`,
`greydot`, `spritecrunch`, `spritedma`, `vicii_timing`, `videomode` and
`screenpos` remain unchanged. This is a measured improvement, not closure of
the `colorfetchbug` category at revision `74f31553`.

Revision `9176e269` resolves the separate bus-ownership question. The survey
now registers all five `colorfetchbug` programs, and each matches all 104,448
classified pixels with an indexed-plane hash identical to its reference. The
clean report is retained at
`target/accuracy/c64-vicii-survey/9176e2690fe25c069fe2b4cb4529a0de4f22f23d/report.json`.
That result closes the selected PAL 6569 colour-fetch contract without
changing this document's display-phase decision.

Revision `d140a36f` resolves the separate far-edge fetch-window length. It
raises `sequencer-bug` from 96,266 to 104,394 matching pixels while all five
exact colour-fetch planes and every other survey plane remain byte-identical.
The clean report is
`target/accuracy/c64-vicii-survey/d140a36f782862706e04b15272bf5f7f4a145862/report.json`.
That result leaves 54 pixels across eight rows for the delayed C-data output
question; it does not change the entering-Phi1 decision specified here.

## Evidence boundary

This decision fixes display-state and matrix-index phase only.

The first-three invalid matrix accesses are no longer an unresolved part of
this decision. Revision `9176e269` supplies an explicit CPU/VIC Phi2 bus
sample, derives AEC from consecutive aggregate BA-low cycles and stores `$FF`
plus the CPU-side low nibble while AEC remains high. That distinct question is
specified by the
[BA-to-AEC handover decision](c64-ba-aec-handover.md).

The implementation does not yet model a distinct draw-side DMLI, a separately
latched graphics buffer or the complete graphics shifter pipeline. Fetch and
render remain combined, so the idle access does not separately update the
VIC-II bus latch. Eight pixels are rendered as one cycle-sized batch. Register
writes whose effects change within those eight dots therefore remain outside
this decision.

All five staged `colorfetchbug` programs are now registered and form a strict
exact-output lane. The selected `border-250` program is still not a complete
oracle for the staged idle-bitmap, multicolour-bitmap and combined horizontal
and vertical-border programs.

Cross-emulator cycle traces rule out an instruction-timing or missing-stall
explanation for the separate `sequencer-bug` case: the apparent lead came
from comparing Emu198x's scheduled pre-tick CPU pins with VICE's post-access
monitor phase. Revision `d140a36f` then removes the excess far-edge c-access
and aligns the following store at cycle 55. The residual is now classified as
delayed C-data output sequencing, not a reason to change the exact display
phase, ownership or window-length results. AEC-sensitive sprite Phi2 bytes 0
and 2 and the effect of invalid activity on `last_bus_data` remain separate
evidence-bounded questions.

The decision is evidenced for the PAL 6569 profile. The implementation uses
the same entering-state split for the existing 6567 variants, but no
equivalent strict 6567R8, 6567R56A or 8565 evidence is registered here.

The staged notes describe C64 observations, but their upstream revision and
full measurement provenance remain unresolved. The clean survey compares
digital palette indices against externally supplied images. Neither source is
a physical-hardware capture package, so this decision does not claim physical
hardware conformance, analogue-video accuracy or a gate-level silicon
explanation.

## Drift triggers

Reject changes that:

- evaluate badline state before the current cycle's Phi1 display decision;
- let a late cycle-16 badline retroactively advance VC or VMLI;
- make the first late matrix access select slot 1 instead of slot 0;
- return to geometry-derived matrix columns instead of live VMLI;
- leave stale framebuffer pixels beneath an opened vertical border;
- apply the vertical under-border fill across the horizontal side-border
  region;
- alter the first three invalid matrix values without preserving the explicit
  CPU/VIC bus source and ownership contract;
- describe the partial revision-`74f31553` survey fraction as a category pass
  or hardware proof;
  or
- extend the PAL phase claim to another VIC-II model without model-specific
  evidence.

Any change to CPU-write scheduling, Phi1/Phi2 ordering, the graphics-buffer
pipeline, border flip-flops or model-specific cycle tables requires the
focused tests and clean survey comparison to be rerun.

## Related Documents

- [C64 accuracy closure campaign](c64-accuracy-closure-campaign.md)
- [C64 architecture review](c64-architecture-review.md)
- [C64 BA-to-AEC handover](c64-ba-aec-handover.md)
- [PAL 6569 far-edge late-badline DMA window](c64-far-edge-badline-window.md)
- [CPU bus interface](cpu-bus-interface.md)
- [Save state format](save-state-format.md)
- [MOS 6569 / 6567 VIC-II](../chips/mos-vic-ii.md)
- [C64 VIC-II reference survey](../processes/c64-vicii-vice-survey.md)
- [VIC-II survey fixture notes](../../test-data/commodore/c64/vicii-vice-survey/README.md)
- [VIC-II VC/VCBASE/RC rewrite plan](../../docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md)
