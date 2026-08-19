# The ZX80's horizontal position is a fixed offset, not a detected sync

**Status:** accepted, with a known limit
**Context:** #1033, #1036, #1037, #295

## Decision

`machine-sinclair-zx80` places each scanline's first character at a constant
`FIRST_CHAR_TSTATE`, measured at 73 T-states from the `HALT` release against
Sinclair's ROM. It does not generate or detect a horizontal sync pulse.

## Why this is worth writing down

Everything else in the model has independent confirmation. Cross-checked
against EightyOne 1.41's source (`Source/zx81config.cpp`, `Source/zx81/zx81.cpp`):

| | EightyOne | here |
|---|---|---|
| clock | 3,250,000 Hz | 3,250,000 Hz |
| T-states per scanline | 207 | 207 |
| lines per frame | 312 | 312 |
| pixels per T-state | 2 | 2 |

Four constants, two implementations, no disagreement. The horizontal origin
is the one thing not confirmed, because EightyOne does not have one: it
tracks `sync_len` and `scanline_len` per line and positions the picture from
the sync pulse it detects, with acceptance windows either side
(`ZX80HSyncAcceptanceDuration`, `ZX80MaximumSupportedScanlineOverhang`).

That is the more faithful model. On real hardware the horizontal position
*is* wherever the sync falls, so a display routine that takes longer to get
from the interrupt back to the display file produces a picture shifted
right — and still a picture.

## What the fixed offset costs

**Only the ROM's own timing renders correctly.** Any other display routine
lands somewhere else, and if it is early the leading characters are clipped
rather than shifted.

This is not hypothetical. The synthetic firmware in #1037 had to be
hand-timed to match the ROM's handler to the T-state:

- loading `A` with the R value inside the handler instead of once outside
  cost 7 T — fourteen pixels of drift
- the same-row and new-row paths had to be equalised with a `RET Z` that is
  never taken, or every eighth scanline stepped ten pixels left and lost two
  characters off the edge

On hardware, none of that would have mattered: the picture would have sat
slightly further right and stayed whole. The emulator forced a precision the
machine does not.

## When to revisit

If a second ZX80 program needs to render — anything not the ROM and not the
#1037 image — model the sync instead of the offset. The shape to copy is
EightyOne's: accumulate a scanline length, watch for the sync, and position
from it, with a tolerance window for routines that are close but not exact.

Until then the offset is honest: it is calibrated, it is documented as
calibrated, and the two things that depend on it both have tests.

## Drift triggers

- "the picture is in the wrong place, adjust `FIRST_CHAR_TSTATE`" — that
  constant is calibrated against the ROM. If new firmware renders in the
  wrong place, the firmware's timing differs and the model is what is
  wrong, not the number.
- writing a ZX80 display routine and finding characters clipped off the
  left — that is this limit, not a bug in the routine
- "our timings might be wrong" — the four that matter are confirmed against
  an independent implementation; check this entry before re-deriving them
