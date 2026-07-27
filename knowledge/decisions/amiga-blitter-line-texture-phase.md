# Decision: Amiga blitter line texture phase

**Date:** July 2026

## The question

How does the line blitter select and advance the pattern bit stored in
`BLTBDAT`?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed page 191,
says to preload `BLTBDAT` with the line texture or `$FFFF` for a solid
line. It says that the B shift value selects the starting texture bit,
with zero meaning the least significant bit. The same setup requires
SRCA, SRCC and DEST while SRCB remains clear. It nevertheless describes
the B input to the minterm as the pattern to draw.

Printed page 192 repeats the register setup: `BLTBDAT` contains the
texture, `BLTCON1` bits 15–12 select its starting bit and SRCB is zero.
This establishes that standard line texture does not depend on B DMA
being enabled.

The inspected WinUAE revision
`69df7fed523f9e79c5641ea4cdfe80eae5c32967` initializes an internal
line-pattern value from `BLTBDAT` and the B shift. It converts the
selected pattern bit to all zeroes or all ones for the minterm, then
decrements the B shift with wrap for the next pixel. Enabling B is
treated as an unusual separate DMA case, not as the gate for preloaded
texture.

The inspected vAmiga revision
`f9e34ca4f199172df77b7109c3fe1f380b87833b` independently loads the
B shifter from the B data value, supplies the selected bit to the line
minterm and decrements the shift on every logical pixel.

## The decision

Line texture always uses the preloaded `BLTBDAT` value. SRCB controls
optional B DMA; it does not enable or disable the standard preloaded
texture.

`BLTCON1.BSH` selects the first bit:

- BSH zero selects bit 0;
- the next logical pixel selects bit 15; and
- subsequent pixels continue downward with 0-to-15 wrap.

The selected texture bit becomes `$FFFF` when set and `$0000` when
clear before it enters the ordinary A/B/C minterm.

Texture phase advances once for every generated line pixel, including
an ONEDOT pixel whose D transfer is suppressed. ONEDOT controls
write eligibility after result generation; it does not pause the
pattern.

`BLTBDAT` remains the preloaded pattern. Advancing the line rotates
internal selector state rather than mutating the data register. The
final selector is reflected in the upper BSH field when the current
line execution state is retired.

## Save-state compatibility

The current texture selector is serialized in the active line runtime.
It cannot be reconstructed from `BLTBDAT` and the line count after an
arbitrary mid-line snapshot because the exact starting phase is also
execution state.

This state is part of Amiga runtime schema version 18. Version 17 is
rejected rather than restoring a line with a different texture phase.

## Model boundary

The implementation covers the standard manual setup with SRCB clear.
It retains the existing C-read and logical-D line schedule.

Optional B DMA in line mode is not yet represented as an additional
bus request. This does not alter the standard preloaded-texture rule.

## Verification

Hermetic tests preload a non-solid pattern with SRCB clear and verify:

- BSH zero consumes bits 0, 15, 14 and 13 in order;
- only the set texture bits produce pixels;
- `BLTBDAT` remains unchanged; and
- the selector has decremented four positions at completion.

The ONEDOT snapshot regression also proves that the serialized texture
phase continues deterministically across restore.

## Drift triggers

Reject these patterns:

- treating SRCB as an enable for the preloaded line texture;
- selecting bit 15 when BSH is zero;
- rotating or overwriting `BLTBDAT` to represent texture progress;
- pausing texture phase when ONEDOT suppresses D; or
- reconstructing the active selector from destination memory.

## Related Documents

- [Amiga blitter line-mode ONEDOT](amiga-blitter-line-onedot.md)
- [Amiga blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
