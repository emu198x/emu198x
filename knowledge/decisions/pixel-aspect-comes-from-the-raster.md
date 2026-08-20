# Pixel aspect comes from the raster, not the crop

**Status:** adopted for the model; cores migrating one family at a time
**Context:** #1053, #1050, `emu198x-shell/src/display.rs`

## Decision

A core's pixel aspect ratio is derived from how fast it emits framebuffer
pixels and how many framebuffer lines a television spreads over its height:

```text
PAR = (4/3) × lines_per_tv_height / (pixel_clock_hz × active_line_seconds)
```

with the active line at 52.0 µs for PAL and 52.6 µs for NTSC. `emu198x_shell::display::pixel_aspect_ratio` is the one implementation.
Cores expose the result through `UiSystem::pixel_aspect_ratio`.

Nothing in that expression mentions the framebuffer's dimensions, and that is
the whole point.

## What it replaces

`UiSystem::display_aspect_ratio` asks a core for the shape its framebuffer
should *fill* — `Some(4.0 / 3.0)` for a television — and the harness divides
that by the framebuffer's own dimensions to get a stretch. That is correct
only when the framebuffer is exactly the picture a set displays, or when both
axes are cropped by the same fraction. Otherwise the crop decides the
geometry: keep a little more border and the proportions change, though the
machine has not.

The ZX80 shows why the old hook cannot be repaired in place. Its window is
320×240, which is already 4:3, so `Some(4.0/3.0)` computes to a stretch of
1.0 and changes nothing — while the true figure is about 1.14. The hook has
no way to say what is true.

## Why the two arguments are the caller's to state

**Pixel clock** is the rate the *framebuffer* fills along a line — not the CPU
clock, and not the dot clock of whichever video mode is selected. A core that
renders every mode into one fixed-width buffer has a single pixel clock
whatever the mode, which is why the BBC's 640-wide buffer has one answer
across all its screen modes.

**Lines per TV height** is the active line count for a progressive core and
twice that for one whose framebuffer holds both interlaced fields. Only the
core knows which it is. The Amiga's 768×576 is the case that forces the
distinction: both terms double against a 384×288 core, and a formula that
assumed one field would be out by two.

## The answer can change under the machine

`UiSystem::pixel_aspect_ratio` takes the runtime, and the harness re-derives
it wherever the picture can change — window creation and variant switching.
Most cores ignore the argument and return a constant, but the shape of the
hook has to allow otherwise, because two things move it:

- **A variant switch can cross regions.** Switching a machine from its PAL
  profile to its NTSC one changes the pixel aspect while the framebuffer keeps
  its dimensions, so a size comparison will not catch it.
- **A display card is not a television.** An Amiga running RTG drives a
  monitor, and the mode's clock is the card's, not the chipset's. `Region` is
  a property of the machine; what this derivation actually needs to know is
  which display is being driven. Those coincide for every core today. If they
  stop coinciding, that is the thing to change — not the formula, which is
  about light on glass and does not care what generated the signal.

## Not every machine has an answer

`Region::Other` returns `None`, and cores fall back to square pixels. A Game
Gear's pixels are square because they are square, not because a standard says
so. Game Boy is the same case and currently reaches square by omission rather
than by saying so; that is worth tidying when it migrates.

## Migration

`display_aspect_ratio` stays until the last core leaves it. The harness prefers
`pixel_aspect_ratio` and falls back, so no core changes behaviour until it is
migrated deliberately. Do not add new cores to the old hook.

Migrated so far: ZX80, ZX81 — both PAL, both two pixels per 3.25 MHz T-state
over 288 active lines, both about 1.14. Twenty-eight remain; #1053 tracks them.
Each migration wants a reference to check against, not just arithmetic.

## Drift triggers

Re-read this entry when you catch yourself writing:

- `Some(4.0 / 3.0)` in a new core
- an aspect ratio computed from `FB_WIDTH` / `FB_HEIGHT`
- "the framebuffer is already 4:3, so square is fine"
- a pixel clock taken from the CPU clock or from the current video mode
- an interlaced core's line count given as one field's worth
