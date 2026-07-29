# Decision: Carry Denise display projection across the Agnus raster wrap

**Date:** July 2026

## The question

Where do pixels produced after Agnus wraps its horizontal position to zero
belong in the displayed raster, and when does Denise start the next line's
line-local state?

## Evidence

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` makes the display projection
explicit in `Core/Components/Agnus/Beam.cpp`. `Beam::pixel` places positions
at or after `HBLANK_MIN` from the start of the current texture row. Positions
below `HBLANK_MIN` are offset by the latched preceding line length, with the
comment that everything left of the horizontal-blank area belongs to the
previous line. `Beam::eol` latches that length before it resets the raw
horizontal position and advances the vertical position.

In the same revision, `HBLANK_MIN` is `$12`. Denise's `hsyncHandler` runs at
raw horizontal position `$12`, finishes and colourises the accumulated line,
then resets its per-line buffers and sprite clipping range. vAmiga therefore
does not treat raw Agnus position zero as the start of a new display row.
The physical line continues across the counter wrap through positions
`$00..$11`.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` independently separates Agnus
line timing from Denise's line-local reset. In `drawing.cpp`,
`do_hbstrt` handles the hardwired horizontal-blank transition and invokes
`hstart_new`. That function resets line-local bitplane counters, the last
bitplane pixel, HAM colour state, the per-line `BPL1DAT` trigger and the
ordinary sprite-hidden reasons. The ECS programmed-horizontal-blank path
invokes the same reset operation from its blank transition rather than from
an unconditional raw horizontal-counter wrap.

The registered vAmiga Amiga Test Kit v1.21 crosshatch reference provides the
end-to-end observation. Before this change, the only remaining strict video
residual was 56 pixels in canonical columns 712–715 and rows 11–271. The
four-column differences occurred on 14 horizontal crosshatch lines. Their
far-right placement corresponds to the first low-resolution output step
after raw PAL raster wrap when projected using the preceding line length.

## The decision

Emu198x distinguishes these two boundaries:

1. Agnus reaches the end of its current raw line, advances `vpos` and wraps
   `hpos` to zero. DMA arbitration and all other clocked machine work continue
   on the new raw line.
2. Denise reaches the horizontal-blank line-start boundary. Only then does
   the board renderer begin the next physical display line and reset
   Denise's line-local display state.

Between those boundaries, Denise output is projected onto the right-hand tail
of the preceding physical row. Its horizontal coordinate is derived from the
latched length of that preceding raw line, followed by the current post-wrap
position. Its row and line-local display context remain those of the
preceding physical line. The current raw `vpos` must not move these pixels
down one framebuffer row.

The board-level Denise wrapper therefore retains a prior-line render context
across the raw wrap. The serialized context preserves the preceding physical
line's `vpos`, actual CCK length, matched DDF origin and derived pipeline row,
vertical-DIW state, raw field identity, and the interlace row selected while
that line was current. The existing line-reset marker records whether the
Denise line-start reset is still pending. These are conceptual contents of
the live context; private field names are not part of the decision contract.
The context is retired when the next physical line begins at the supported
hardwired horizontal-blank boundary.

This is a display projection and state-lifetime rule. It does not change
Agnus time, insert a bus opportunity or replay work from the preceding line.
In particular, the renderer must not manufacture a synthetic terminal CCK to
obtain the missing pixels. The real post-wrap CCK remains the sole source of
its DMA, Copper, CPU-arbitration and Denise effects; only its destination in
the displayed raster is carried backward.

The carry is live machine state. Runtime postcard snapshots preserve it so a
restore between the raw wrap and the Denise line-start boundary cannot place
the remaining pixels on a different row or reset the pixel pipeline early.
Snapshot schema version 27 rejects version 26 because the older positional
payload cannot represent this context.

## Model boundary

The first implementation covers the normal hardwired OCS PAL and NTSC line
shape used by the current board renderer and registered Test Kit lane.

ECS and AGA can program horizontal blanking. WinUAE's separate ECS and AGA
paths show that their reset conditions are not reducible to the fixed OCS
boundary. Exact `BEAMCON0`, `HBSTRT`, `HBSTOP`, composite-sync and chipset
revision interactions remain separate work. Until those signals are exposed
to the shared renderer, this decision must not be read as complete
programmable-horizontal-blank behaviour.

Frame publication follows the same ownership. At the final raw line of a
field, post-wrap output still belongs to that field after Agnus increments its
raw field counter. The runtime therefore observes a display-complete field
count which holds that increment back until Denise retires the carried row at
the supported HBLANK boundary. Fixed-sync PAL and NTSC output, including
interlaced fields, is not published with a stale right edge. Programmable
horizontal blanking remains subject to the model boundary above.

The decision does not alter the sprite horizontal comparator, the
one-low-resolution-pixel sprite output delay, bitplane fetch arbitration,
Copper comparison coordinates or line-length selection. Those operations
continue on their existing clock authorities.

## Verification

Hermetic board-level and runtime tests establish that:

- the first real post-wrap low-resolution step is written at the far-right
  tail of the preceding physical row;
- PAL and NTSC placement uses the actual preceding line length;
- raw `hpos == 0` does not reset Denise's line-local state;
- the supported hardwired horizontal-blank boundary performs one reset and
  starts the next physical row;
- the line reset precedes a coincident enhanced-chipset wide bitplane fetch;
- no additional CCK, DMA grant or Copper step is introduced; and
- snapshot round trips preserve an in-flight carry while version-26 payloads
  are rejected.

The runtime publication test gives the terminal PAL line a non-black COLOR00
and verifies that both emitted framebuffer rows are current through board
column 767. This covers the portion beyond the registered Test Kit crop.

The strict Amiga Test Kit v1.21 lane passes after implementation. The
pre-change crosshatch residual was 56 pixels with bounding box
`x=712..715, y=11..271`; the post-change crosshatch comparison is exact.
Gradients, the static checkerboard, both alternating-checkerboard phases, EBU
bars and dots also remain exact.

## Drift triggers

Reject these patterns:

- mapping every pixel solely from the current raw `vpos`;
- resetting Denise line-local state unconditionally at raw `hpos == 0`;
- deriving the previous line's length from the new line after a PAL, NTSC or
  long/short-line transition;
- fixing the right edge by adding a synthetic end-of-line CCK;
- moving Agnus arbitration or Copper time to match framebuffer coordinates;
- reconstructing an in-flight carry during snapshot restore; or
- claiming fixed OCS reset timing implements programmable ECS or AGA
  horizontal blanking.

## Related documents

- [Amiga sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md)
- [Denise BPL1DAT sprite visibility](amiga-denise-bpl1dat-sprite-visibility.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
