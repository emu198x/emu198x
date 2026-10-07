# Decision: Delay Amiga sprite output after the horizontal comparison

**Date:** July 2026

## The question

When does Denise emit the first sprite pixel relative to the horizontal
position decoded from `SPRxPOS` and `SPRxCTL`?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed pages 123–126,
states that writing `SPRxDATA` arms the next horizontal comparison, that the
comparison loads the sprite's parallel-to-serial converter, and that the
converter shifts once per low-resolution pixel. It defines the register
coordinate and the two operations, but does not expose their ordering within
one low-resolution pixel period.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` states in `drawing.cpp` that a
sprite start always has a one-low-resolution-pixel delay. Its horizontal match
copies `SPRxDATA` and `SPRxDATB` into the serial state and arms the shifter.
The renderer contributes the previously latched sprite code before it loads
the next most-significant bits, so newly loaded data cannot appear on the
comparison pixel.

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` decodes the OCS horizontal
coordinate and then adds two hires pixels to its display position. Two hires
pixels are one low-resolution sprite pixel. This independently agrees with
WinUAE's pipeline ordering.

The registered vAmiga Amiga Test Kit v1.21 references expose the same
observable OCS placement end to end. Before the correction, the menu pointer
was two canonical hires pixels left of the reference while the surrounding
playfield pixels aligned. Its shape and colours otherwise matched.

## The decision

Denise compares the live horizontal counter with the nine-bit OCS coordinate
decoded as:

```text
HSTART = (SPRxPOS[7:0] << 1) | SPRxCTL[0]
```

Within Emu198x's per-low-resolution-pixel sequencer, a match copies armed
`SPRxDATA` and `SPRxDATB` into the sprite shift registers. Composition does
not observe the newly loaded code on that step. On OCS/ECS, the first most-significant
sprite-data bits reach display and collision logic on the following step.
Lisa retains those codes for one further step as described below.
This preserves the decoded comparator coordinate while reproducing the
independently observed output placement.

The delay belongs to the sprite shifter. It is not represented by changing
the decoded register value, offsetting the framebuffer, or moving only the
visible compositor result. Sprite-to-sprite and sprite-to-playfield collision
codes therefore advance with the displayed sprite.

Writing `SPRxCTL` still disarms the horizontal comparison. Writing
`SPRxDATA` still arms it. Moving an armed sprite by writing `SPRxPOS` changes
the future comparison coordinate, not the one-pixel output phase.

## Model boundary

The current implementation clears each sprite's contributed code for the
comparison step, loads its serial registers, and begins shifting on the next
step. This establishes the externally observed start phase. The manuals'
coordinate-language does not by itself distinguish an internal silicon delay
from the mapping between Denise's counter and displayed pixels; the decision
therefore fixes observable sequencing in this emulator rather than asserting
an unseen gate-level implementation.

WinUAE also distinguishes the previously latched output code during unusual
same-position data rewrites and same-line reloads. Those finer reload cases
remain separate accuracy work; this decision does not claim that the current
single-stage shifter reproduces every internal latch.

The neutral fixed-lores sprite program measures the sprite relative to a
bitplane marker on the same scanline. vAmiga and FS-UAE both report 16 hires
samples on OCS; FS-UAE also reports 16 on AGA. Emu198x originally reported
16 on OCS and 14 on AGA. The
[primary observation record](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md)
records the programs, profiles and producer boundaries.

Lisa therefore retains the generated sprite code for one additional lores
output step, matching its later bitplane output phase. The horizontal comparison
and serial load still use the shared decoded HSTART. Composition and collision
logic consume the retained code together. The stage is serialized and cleared
at a beam-line reset. This is an observable phase calibration, not a claim
about an unseen physical Lisa latch. ECS superhires position bits, AGA sprite
resolution modes, and unusual same-line rewrites remain separate questions.

## Verification

Hermetic Denise tests establish that:

- the decoded `HSTART` pixel is background;
- OCS first newly loaded sprite data appears at `HSTART + 1`;
- Lisa compares and loads at HSTART, with visible and collision code at `HSTART + 2`;
- pending Lisa sprite code survives save/restore and leaves no collision trail;
- `SPRxCTL` bit 0 still selects odd horizontal comparison coordinates;
- collision state is absent at `HSTART` and begins with visible sprite data at
  `HSTART + 1`;
- sprite priority and attached-pair composition are tested on the delayed
  output coordinate; and
- 32- and 64-pixel sprite test offsets include the same one-pixel start phase.

The explicit Amiga Test Kit v1.21 video lane verifies the correction against
the independently produced A500+A501 OCS PAL reference. Gradients, the static
checkerboard, both alternating-checkerboard phases, and dots now match exactly.
In the phase-only run, the EBU-bars case retained 114 pointer-region pixels.
That residual led to the separate
[Denise BPL1DAT sprite-visibility](amiga-denise-bpl1dat-sprite-visibility.md)
decision and is not evidence for another HSTART offset.

After implementing that separate prerequisite, the OCS pointer placement
matches in the non-colour cases. The phase-only crosshatch result retained 56
far-right pixels caused by post-wrap raster placement, and the same 56 pixels
remain after the visibility change. The separate
[Denise raster-wrap projection](amiga-denise-raster-wrap-projection.md)
decision removes that residual rather than hiding it with a sprite or
framebuffer offset. Current gradients and EBU-bar status is governed by the
separate Copper colour-phase disagreement, not by the sprite coordinate.

## Related documents

- [Denise BPL1DAT sprite visibility](amiga-denise-bpl1dat-sprite-visibility.md)
- [Denise raster-wrap projection](amiga-denise-raster-wrap-projection.md)
- [Lisa bitplane and display-window output phase](amiga-lisa-bitplane-diw-output-phase.md)
- [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Sprite horizontal-phase conformance corpus](../../test-data/commodore/amiga/sprite-horizontal-phase/README.md)

Both current Test Kit profile contracts require exact agreement for every case.
The AGA pointer discrepancy is resolved without moving reference crops.

## Initial AGA hires sprite sampling (v40)

The [primary diagnostic record](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#aga-sprite-resolution-diagnostic)
reproduced missing SPRES and fractional-position handling with real DMA:
explicit hires sprites were 32 hires samples wide instead of 16, and SPRxCTL
bit 4 did not move the start by one hires sample. The lores baseline agreed.

The v40 implementation introduced the hires sprite selector independently of the playfield
clock. Explicit SPRES=10 selects hires; automatic SPRES=00 selects hires for
a superhires playfield and lores for lores/hires playfields. SPRES=01 remains
lores. The shared OCS/ECS sequencer stays at its original lores granularity.

Lisa's serializer advances at two hires sample boundaries per lores board
tick. A horizontal match starts the shared one-lores-period load stage;
thereafter it emits one bit per selected sprite pixel period. A two-hires
sample queue retains the output before composition. This preserves the
measured two-lores-period start separation in both calibrated rates. It
models observed sequencing, not an assertion about unseen silicon latches.
SPRxCTL bit 4 adds one hires sample to the comparator coordinate.

Each simultaneous sprite code participates in priority and collision
matching against the playfield sample at that time. A lores playfield can
therefore have two different winning sprite colours in one lores period.
Superhires playfield samples share the appropriate hires sprite sample;
the board's hires framebuffer retains source positions 0 and 2 from each
four-sample group. Lisa continues advancing when the output is hidden.
The held code, clock countdown and output queue are serialized. Runtime
snapshot format v40 rejects v39 states, whose layout lacks those stages.

Production A1200 DMA captures now match all three calibrated probes: lores
width/start 32/16 hires samples, hires 16/16, and hires with the fractional
position bit 16/17. Tests also cover alternating and isolated bits over all
three playfield resolutions, automatic fallback, simultaneous sprite
priority/collisions and restoring partially emitted 16/32/64-bit streams.
Evidence and checks are in `target/amiga-hires-sprite-validation/`.

## AGA superhires and quarter-position extension

The [full-superhires primary diagnostic record](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md#aga-full-superhires-sprite-capture--2026-10-05)
now retains every 35 ns sample. Thirteen DMA probes at three adjacent fields
confirm explicit SPRES=11 emits sixteen bits in sixteen samples, with CTL
bits 3/4 moving the start by one/two samples. Patterned A5A5 probes confirm
serial bit order and holding periods, including fractional lores/hires output.

Lisa now advances four 35 ns samples per lores call. SPRES selects a serial
period of four, two or one samples; automatic selection remains lores for
lores/hires playfields and hires for superhires playfields. Comparator position
includes CTL bits 4 and 3. The common load stage and retained output queue
both remain one lores period (now four samples each), preserving all earlier
lores/hires timing. Priority and collision logic consume simultaneous codes
at each of the four samples. OCS/ECS retain their existing sequencer.

Runtime snapshot v41 rejects v40 because the serialized sprite beam coordinate,
countdown and packed queue now have different units. Regression tests cover
all sprite rates, all four fractional positions, three playfield resolutions,
solid/alternating/isolated bits, adjacent versus overlapping sprite-group
collisions, and restoring pending 16/32/64-bit superhires streams.

The [full Lisa framebuffer](amiga-full-superhires-framebuffer.md) now retains
all four samples through palette/HAM resolution, final blanking and native
frame/monitor transport. The earlier hires-only transport selected samples 0
and 2. The serializer does not reproduce the old reference frontend's
nine-hires-sample footprint by inventing a wider physical sprite.

Artifacts and validation live in `target/amiga-superhires-sprite-validation/`.
Mid-line selector propagation, unusual reloads and original-hardware timing
remain separate calibration boundaries.

## Counter-origin correction (2026-10-06)

The counter-origin investigation supersedes the additional Lisa output tick
and HSTART+2 claims above. Four independent DMA controls (16/32/64-bit fetch
modes, fractional superhires position) locate their first sprite sample at
HSTART+1 lores period in the UAE counter domain. The native extra history tap
placed every sprite pattern four 35 ns samples late. The user approved tracing
and correcting this stage as part of the Lisa phase recalibration.

The shifter still loads at HSTART and waits one lores period. Its held code now
feeds composition and collisions directly. Legacy history fields remain in
snapshot 53. This changes no register coordinate or native framebuffer origin.
See `test-data/commodore/amiga/ecs-output-phase/lisa-correction/` for the failing
controls and completed validation; these are software observations.
