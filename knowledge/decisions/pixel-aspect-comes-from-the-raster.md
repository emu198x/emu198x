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

## `display_aspect_ratio` keeps one honest job

The old hook is not simply the pre-migration way of doing this. It asks a core
what shape its framebuffer should *fill*, and that is the right question for a
display that shows the whole framebuffer.

A television does not. It overscans, which is why the raster derivation asks
how much of a broadcast line a set displays and takes no framebuffer
dimensions at all. A dedicated monitor has no such convention: the PET's 4:3
monochrome screen shows the raster it is given, so "stretch this buffer to
4:3" is exact rather than approximate.

So the Commodore PET stays on `display_aspect_ratio`, and it is the only core
that should. Its profile says `Region::Other` for the same reason and the
raster derivation would decline to answer it. Every core that drove a
television belongs on `pixel_aspect_ratio`; the hook is not deleted when the
last of them moves.

The harness prefers `pixel_aspect_ratio` and falls back, so no core changes
behaviour until it is migrated deliberately.

Migrated so far — sixteen of thirty:

| Core | TV | PAR | Published |
|---|---|---|---|
| ZX80, ZX81 | PAL | 1.136 | — (measured against MAME 0.289) |
| Spectrum 16K/48K/+/Pentagon | PAL | 1.055 | — |
| Spectrum 128K/+2/+2A/+2B/+3 | PAL | 1.041 | — |
| Timex TS2068 | NTSC | 0.870 | — |
| MSX, ColecoVision, SG-1000, Sord M5, SVI-328, MTX, Einstein | NTSC | 1.1429 | 8:7 ✓ |
| " | PAL | 1.382 | — |
| Master System | NTSC | 1.1429 | 8:7 ✓ |
| NES | NTSC | 1.1429 | 8:7 ✓ |
| C64 | PAL | 0.9369 | 0.9365 ✓ |
| C64 | NTSC | 0.7500 | 0.7500 ✓ |
| Atari 2600 | NTSC | 1.7143 | 12:7 ✓ |
| Atari 2600 | PAL | 2.0820 | 25:12 ✓ |
| Game Boy, Game Gear | — | 1.0 | not televisions |

What each core showed *before* is not one story, and commit messages from the
migration get this wrong — several say "square" of cores that were not:

| Core | Was | Now | Shift |
|---|---|---|---|
| ZX80, ZX81, Spectrum, NES, C64 | 1.0 — square, by never overriding the hook | derived | up to +14% (NES) |
| MSX, ColecoVision, SG-1000, Sord M5, SVI-328, MTX, Einstein, Master System | 1.1111 — `Some(4/3)` over a 288×240 buffer | 1.1429 | +2.9% |
| Atari 2600 | 1.8667 — `Some(4/3)` over a 160×224 buffer | 1.7143 | **−8.2%** |

The 2600 is the one to remember: it was the furthest out of any core, and the
correction makes its picture *narrower*, not wider. Reading the old hook as
"square" is the mistake — it was stretching to 4:3, which lands somewhere
different for every framebuffer shape.

Then the rest of the fleet:

| Core | TV | PAR | Published |
|---|---|---|---|
| Atari 800XL, 5200, 7800 | NTSC | 0.8571 | 6:7 ✓ |
| " | PAL | 1.041 | — |
| Aquarius | NTSC | 0.8571 | 6:7 |
| VIC-20 | NTSC | 0.7500 | matches the NTSC C64 exactly |
| VIC-20 | PAL | 0.8328 | ≈ 5/6 |
| Atom, Dragon | PAL | 1.041 | — |
| Oric | PAL | 1.231 | — |
| Jupiter Ace | PAL | 1.136 | matches the ZX80 exactly |
| BBC Micro, Electron, CPC | PAL | 0.4615 | — |
| Amiga (hires, interlaced) | PAL | 1.041 | — |

Twenty-nine of thirty cores now derive it; the PET is the thirtieth and stays
where it is, for the reason below.

Two of these check themselves against work done earlier. The VIC-20 on NTSC
lands on 0.7500, the same as the NTSC C64 — both fetch a character per cycle
and emit eight pixels, at the same cycle rate, so they must agree, and they
do. The Jupiter Ace lands on the ZX80's 1.136: same 207 T-states, same 312
lines, same two pixels per 3.25 MHz T-state. Neither was arranged; both fall
out of clocks read from their own cores.

## This says nothing about overscan

Pixel aspect is the shape of a pixel. How much of the raster a core keeps is a
separate axis, and none of it is settled by the work above — the derivation
takes no framebuffer dimensions, which is exactly why it could be fixed
without touching any crop.

The migration does leave the instrument to measure the other axis, because
every migrated core now states a pixel clock. A set's window is
`pixel_clock × active_line_seconds` wide and `active_lines` tall, so:

| Core | Framebuffer | Set's window | Horizontal | Vertical |
|---|---|---|---|---|
| ZX80, ZX81 (PAL) | 320×240 | 338×288 | 95% | 83% |
| Spectrum 48K (PAL) | 352×296 | 364×288 | 97% | 103% |
| TMS9918 (NTSC) | 288×240 | 280×240 | 103% | 100% |
| TMS9918 (PAL) | 288×240 | 278×288 | 104% | 83% |
| NES (NTSC) | 256×240 | 280×240 | 91% | 100% |
| C64 (PAL) | 416×312 | 410×288 | 101% | 108% |
| Atari 2600 (NTSC) | 160×224 | 187×240 | 86% | 93% |

Under 100% has two quite different causes and this table cannot tell them
apart. The NES renders only 256 dots and blanks the rest, so a set genuinely
shows black at the sides — that is the hardware, not our crop. The ZX80's 83%
is our crop, and [`../../`#1054] is about exactly that. Over 100% means we
present raster a set would hide.

Sorting one from the other needs per-core knowledge of what the chip renders
against what it blanks. Nobody has done that pass, and the spread here — 86%
to 104% horizontally, 83% to 108% vertically — is the argument for doing it.

## Calibrating the active line

The two active-line constants are the only free parameters, and they are
pinned by published ratios rather than chosen.

`NTSC_ACTIVE_LINE_SECONDS` was first set to 52.6 µs, the broadcast active
video interval. That put every NTSC machine about 0.9% out. Working backwards
from four published ratios — the C64's 0.7500, the NES's and the TMS9918's
8:7, and the Atari 2600's 12:7 — gives 52.148 µs from all four, to three
decimals, across four chips with four different clocks. Four independent
sources converging on one figure is a measurement, not a convention, and a
domestic set overscanning a little is the physical reason it sits below the
broadcast number.

The same exercise on PAL gives 52.02 µs from the C64 and 51.97 µs from the
2600; 52.0 sits between them and reproduces both inside a tenth of a percent.

A change to either constant should have to say which published ratio it is
prepared to break. `the_published_ntsc_ratios_all_land` is that check.

## Region is the clock, not the glass

The Game Gear's profile reports `Region::Ntsc`. That is true of its timing and
says nothing about its display, which is an LCD — deriving a TV aspect from it
would stretch a picture that never went near a television. It returns `1.0`
explicitly, and says why.

The Game Boy reaches the same answer by a different route: its profile says
`Region::Other`, so the derivation declines. It states `1.0` anyway, so that
square is a decision rather than an omission.

This is the same seam as the RTG note above, arrived at from the other end:
`Region` describes the signal a machine generates, and the derivation needs to
know what displays it.

## Drift triggers

Re-read this entry when you catch yourself writing:

- `Some(4.0 / 3.0)` in a new core
- an aspect ratio computed from `FB_WIDTH` / `FB_HEIGHT`
- "the framebuffer is already 4:3, so square is fine"
- a pixel clock taken from the CPU clock or from the current video mode
- an interlaced core's line count given as one field's worth
