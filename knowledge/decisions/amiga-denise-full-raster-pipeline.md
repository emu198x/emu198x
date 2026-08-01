# Decision: Advance the Denise pipeline across the full projected raster

**Date:** 2026-08-01
**Status:** BINDING

## The question

Which raster ticks advance Denise and Lisa display-pipeline state when only a
fixed portion of the raster is retained in the host framebuffer?

## Evidence

Amiga Test Kit v1.21 builds its crosshatch from one low-resolution bitplane.
The case programs `BPLCON0=$1200`, `DIWSTRT=$1B51`, `DIWSTOP=$37D1`,
`DDFSTRT=$0020` and `DDFSTOP=$00D8`. Its horizontal grid rows contain set bits
across the complete fetched width, so the pattern distinguishes bitplane
pipeline advancement from palette and Copper timing.

The previous board renderer began running the Denise compositor only at its
fixed retained-viewport origin, CCK `$2C`. With the Test Kit's early data-fetch
start, the first `BPL1DAT` word arrives at CCK `$27` and the next at `$2F`.
Because the bitplane parallel-copy and serial-shift stages did not run before
`$2C`, the second fetch could overwrite the pending first word before that word
contributed its remaining pixels.

The resulting signature was exact and repeatable: canonical columns `x=0..9`
were black instead of white on each of the 14 horizontal crosshatch rows, for
140 differing pixels. The later vertical grid lines already aligned, ruling
out a general crop or horizontal-position correction.

## The decision

Every tick for which Agnus supplies a physical display projection advances the
complete Denise or Lisa output pipeline. This includes:

- pending bitplane parallel copies and serial shifts;
- sprite comparison, shift, priority and collision state;
- HAM hold and modification state; and
- deferred colour-output timing.

The display-window and bitplane-fetch coordinates for that tick determine what
the pipeline contributes. The fixed host viewport determines only whether the
already-produced pixels are stored. Clipping framebuffer writes must not pause
chip state.

Each logical output sample is resolved once. When the runtime duplicates a
non-interlaced line into two host rows, it copies the resolved colour to both
rows. It does not advance HAM, palette or sprite state separately for each
row.

Before the hardwired horizontal-blank start, startup can have no retained
display projection because there is no preceding physical-line context. For
such `projection=None` ticks, the renderer does not fabricate raster
coordinates or advance bitplane and sprite state. It advances only deferred
colour timing, whose delay is defined in output samples and must expire even
when no host pixel is retained.

This rule changes neither Agnus arbitration nor the time of a DMA fetch. It
only separates physical chip-pipeline advancement from host-framebuffer
storage.

## Consequences

An early DDF window can load and shift bitplane data before the fixed host
viewport begins. By CCK `$2F`, the word fetched at `$27` has already entered
the serial pipeline, so the following fetch cannot erase its visible tail.
The first ten canonical hires pixels of the Test Kit's horizontal crosshatch
rows are therefore preserved.

HAM state, sprite shifters, collisions and Lisa's delayed palette output obey
the same physical-raster ownership. Adding a separate pre-viewport bitplane
special case would leave those other stateful paths incorrect.

## Model boundary

The decision governs ticks for which the shared board renderer has a valid
physical projection. How Agnus constructs that projection across raw counter
wrap and programmable horizontal blanking is defined separately. The renderer
must consume the supplied context; it must not infer a preceding line where
none exists.

The registered comparison is one FS-UAE 5.0.7, WinUAE-derived software-family
observation on an A1200 AGA PAL configuration. The passing A500 lane adds a
separate vAmiga-family OCS regression boundary, but neither result is a
physical-hardware measurement or general proof of every Denise and Lisa mode.

## Verification

A focused board regression uses the Test Kit's early-DDF ordering and proves
that the first `BPL1DAT` word advances before the later fetch can replace the
pending stage. A separate row-duplication regression proves that two host rows
do not advance HAM twice.

The strict A1200 AGA PAL Test Kit lane matches its registered reference exactly
for gradients, the static checkerboard, both alternating-checkerboard phases,
EBU bars, dots and crosshatch. The crosshatch changes from 140 differences in
`x=0..9` across 14 rows to an exact result. The strict A500+A501 OCS PAL lane
also remains exact for all registered cases.

## Drift triggers

Reject these patterns:

- calling the compositor only for pixels retained in the host framebuffer;
- advancing only bitplane state outside that viewport;
- deriving chip timing from a crop or image-alignment offset;
- resolving HAM or palette output once per vertically duplicated host row;
- inventing raster coordinates when no physical projection exists; or
- changing Agnus fetch arbitration to compensate for a renderer-side pause.

## Related Documents

- [AGA Lisa colour-output delay](amiga-lisa-color-output-delay.md)
- [Carry Denise display projection across the Agnus raster wrap](amiga-denise-raster-wrap-projection.md)
- [Denise BPL1DAT sprite visibility](amiga-denise-bpl1dat-sprite-visibility.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
