# The ZX80's raster free-runs and is locked by sync

**Status:** superseded in part — the free-running clock landed; the fixed
back porch remains
**Context:** #1033, #1036, #1037, #295

## Decision

The line clock free-runs at 207 T-states and is *locked* by the horizontal
sync — the interrupt acknowledgement that releases the `HALT`. A sync is
authoritative when it arrives; the clock only decides how long to hold a
line open waiting for one, allowing 40 T-states of overrun before giving up.
That is a television's flywheel, and it is EightyOne's model.

Within a line, the first character is placed at a constant
`FIRST_CHAR_TSTATE`, measured at 73 T-states from the sync. That back porch
is still fixed rather than derived.

## What changed, and what it bought

The original decision counted lines *only* from `HALT` edges. That is fine
for firmware that syncs every line, and stops dead for anything that does
not. The free-running clock removes that dependency.

**It changed nothing observable.** Cross Chase renders identically before
and after, and so does everything else. The entry previously predicted that
only the ROM's own timing would render correctly; that prediction was not
borne out — Cross Chase's 8K build draws its title screen correctly under
both models, because it uses the ROM's display routine rather than its own.
The free-running clock is a correctness change against the hardware and the
reference implementation, not a fix for an observed fault.

It cost a one-line phase shift: `display_line` now advances when the `HALT`
releases rather than when it is entered, which moved the synthetic test
image down a line and needed its border retuned from 48 to 47.

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

## What the fixed back porch still costs

A display routine whose timing differs from the ROM's lands somewhere else
horizontally, and if it is early the leading characters are clipped rather
than shifted off the left as they would be on a set.

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

## When to revisit the back porch

If a program renders in the wrong place *horizontally*. EightyOne goes
further than we do here: it measures each sync pulse and classifies it by
duration — short is a line, long is a field — rather than taking the
interrupt acknowledgement as the line sync and an `OUT` as the field sync.
Copying that would remove the last fixed constant.

Nothing has needed it yet. Two real programs load and one renders; the
other, Cross Chase's 16K build, runs without drawing at all, which is a
different problem and tracked separately.

## Drift triggers

- "the picture is in the wrong place, adjust `FIRST_CHAR_TSTATE`" — that
  constant is calibrated against the ROM. If new firmware renders in the
  wrong place horizontally, the firmware's timing differs and the model is
  what is wrong, not the number.
- "lines are being counted twice, tighten the free-run threshold" — the
  40 T-state overrun exists so a line that syncs slightly late is not
  counted once by the clock and once by the sync. Tightening it halves the
  picture; that was tried.
- writing a ZX80 display routine and finding characters clipped off the
  left — that is this limit, not a bug in the routine
- "our timings might be wrong" — the four that matter are confirmed against
  an independent implementation; check this entry before re-deriving them
