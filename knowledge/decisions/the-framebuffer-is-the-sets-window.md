# The framebuffer is the set's window

**Status:** rule stated; ZX80 and ZX81 conform vertically, the fleet does not
yet
**Context:** #1054, #1053, `knowledge/decisions/pixel-aspect-comes-from-the-raster.md`

## Decision

A core whose `Display` is a `Television` should present the window a set
displays: no more, and no less.

```text
width  = pixel_clock_hz × active_line_seconds
height = lines_per_tv_height
```

Both terms are already stated by `Display::Television`, so the correct size is
derivable rather than chosen. Everything outside that window is invisible on
the hardware. Everything inside it is visible — including border the machine
generates, and including black where the machine blanks.

## Why "no less" as well as "no more"

Cropping tighter than the window looks harmless, because the extra is usually
border. It is not harmless on a machine whose picture can move.

The ZX80's vertical position is software-timed: firmware counts border lines
and then jumps into the display file, so a routine that counts differently
puts the picture somewhere else, and it blanks entirely during `LOAD`. A
240-line window over a 312-line frame clipped that movement while a set would
have shown it. Widening to the set's own 288 lines is not extra border for its
own sake — it is the difference between modelling the behaviour and hiding it.

The test is `a_software_moved_picture_stays_whole`, which rewrites the
firmware's border count and checks the picture both moved and survived. It
fails against the old window, which is the only reason to trust it.

## Size is derivable; position is not

The window's *size* falls out of the clock. Its *position* does not, and this
is where the rule stops short of being mechanical.

Vertically the ZX80 anchors honestly: frame line 0 is the end of the vertical
sync pulse, so the vertical interval that follows it is what a set blanks —
312 lines less 24 is 288, and the arithmetic closes without borrowing a figure
from anywhere else.

Horizontally it does not. `FIRST_CHAR_TSTATE` is a fitted constant, calibrated
to place the picture inside a window we had already chosen, so deriving a
set's horizontal window from it would be circular. Fixing that needs a
measurement against a reference, not arithmetic — MAME 0.289 puts its window
24 T-states earlier than ours, which is a starting point rather than an
answer. The ZX80 keeps its 320-pixel width until someone measures it.

## Under 100% has two causes

An audit that compares framebuffers against set windows cannot, on its own,
tell a crop from a chip that renders less:

- **The chip renders less.** The NES draws 256 dots and blanks the rest of the
  line, so a set genuinely shows black at the sides. The framebuffer is the
  whole picture and nothing is wrong.
- **We cropped.** The ZX80's old 83% was ours.

Separating them needs each chip's rendered-against-blanked extent. That pass
is not done, and #1054 tracks it.

## What blocks the rest of the fleet

Applying this fleet-wide wants two things that do not exist yet.

**An instrument.** No crate depends on all thirty frontends, so nothing can
enumerate them and compare. `Display` also lives in the UI layer, where
headless and MCP cannot see it —
[`pixel-aspect-comes-from-the-raster.md`](pixel-aspect-comes-from-the-raster.md)
records that it moves to the profile when something needs to query it, and
this is that need.

**Goldens.** Changing a framebuffer's dimensions invalidates every frame hash
and screenshot taken against it. The ZX80 and ZX81 could move because neither
has committed image goldens. The catalogue systems cannot move so cheaply.

## Drift triggers

Re-read this entry when you catch yourself writing:

- a framebuffer size chosen to fit the picture snugly
- "the extra is only border, so cropping it is free"
- a border constant picked to look right in a screenshot
- an extent audit that reads under-100% as a crop without checking what the
  chip blanks
