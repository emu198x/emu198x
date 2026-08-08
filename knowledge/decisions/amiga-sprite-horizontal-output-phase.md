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
not observe the newly loaded code on that step. The first most-significant
sprite-data bits reach display and collision logic on the following step.
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

ECS superhires sprite-position bit behaviour and AGA sprite-resolution modes
are also outside this OCS phase decision. The A1200 Test Kit reference retains
a separate two-host-HIRES-sample pointer observation under its beam-absolute
crop. Inspected UAE source still specifies the same one-lores-pixel start delay
for OCS/ECS and AGA, so Emu198x does not add a Lisa-only offset from that image
alone. A machine-neutral sprite-phase probe is the next evidence step.
The project-authored
[sprite horizontal-phase conformance corpus](../../test-data/commodore/amiga/sprite-horizontal-phase/README.md)
fixes that register program and capture contract without declaring an expected
AGA offset.

## Verification

Hermetic Denise tests establish that:

- the decoded `HSTART` pixel is background;
- the first newly loaded sprite pixel appears at `HSTART + 1`;
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
