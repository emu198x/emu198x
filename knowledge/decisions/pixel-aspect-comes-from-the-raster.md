# Pixel aspect comes from the raster, not the crop

**Status:** adopted; all thirty cores state a display
**Context:** #1053, #1050, `emu198x-shell/src/display.rs`

## Decision

A core's pixel aspect ratio is derived from how fast it emits framebuffer
pixels and how many framebuffer lines a television spreads over its height:

```text
PAR = (4/3) × lines_per_tv_height / (pixel_clock_hz × active_line_seconds)
```

with the active line at 52.0 µs for PAL and 52.148 µs for NTSC — both
calibrated, not chosen; see below. `emu198x_shell::display::pixel_aspect_ratio` is the one implementation.
Cores expose this through `UiSystem::display`, which states what the output
reached; see below.

Nothing in that expression mentions the framebuffer's dimensions, and that is
the whole point.

## Why the old hook could not be repaired in place

The ZX80 is the case that forces a new hook rather than a better constant.
`display_aspect_ratio` asked what shape the framebuffer should *fill*, and the
harness divided by the buffer's own dimensions. The ZX80's window is 320×240,
already 4:3, so `Some(4.0/3.0)` computes to a stretch of 1.0 and changes
nothing — while the true figure is about 1.14. The hook had no way to say what
was true.

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

`MachineCore::display` takes `&self`, and the harness re-derives
it wherever the picture can change — window creation and variant switching.
Most cores ignore the argument and return a constant, but the shape of the
hook has to allow otherwise, because two things move it:

- **A variant switch can cross regions.** Switching a machine from its PAL
  profile to its NTSC one changes the pixel aspect while the framebuffer keeps
  its dimensions, so a size comparison will not catch it.
- **The display can change without the machine changing.** An Amiga running
  RTG drives a monitor from a chipset that also feeds a set, at a clock the
  card picks. `Display` says which of those is in front of you, so this is a
  different return value rather than a different formula — the arithmetic is
  about light on glass and does not care what generated the signal.

## One hook: what the output reached

`MachineCore::display` returns a `Display`, and the harness derives the pixel
aspect from it. There is no second hook.

It sits on the machine rather than the UI because a display is a fact about
the hardware, and stating it there puts it on the shared query surface —
`session.display.kind`, `session.display.pixel_clock_hz`,
`session.display.lines_per_tv_height` — where headless and MCP can read it.
`UiSystem::display` defaults to delegating, so the window and an audit see the
same answer. It is not on `MachineProfile` because it can move under a running
machine, and because one runtime can serve two machines: `SmsRuntime` is a
television as a Master System and a panel as a Game Gear.

```rust
enum Display {
    Television { region, pixel_clock_hz, lines_per_tv_height },
    Lcd,
    Monitor { aspect },
}
```

The three derive geometry in genuinely different ways, and the type exists to
stop them being confused:

- A **television** overscans — it shows a fixed slice of each line whatever the
  machine sends — so its geometry comes from the raster and **cannot** depend
  on how much of the signal we kept.
- A **monitor** displays the raster it is handed, so the framebuffer *is* the
  picture and its dimensions decide the shape. This is the one case where
  reading the framebuffer is correct rather than the bug this entry is about.
- An **LCD** has square pixels because its pixels are square.

`Display::pixel_aspect_ratio` takes the framebuffer dimensions and ignores them
for two variants out of three. That asymmetry is the point, and
`only_a_monitor_reads_the_framebuffer` pins it.

### What this replaced

Two hooks and three special cases. `display_aspect_ratio` asked what shape the
framebuffer should fill and the harness divided by the buffer's own shape —
right for a monitor, wrong for a set. `pixel_aspect_ratio` replaced it for
televisions but could not say "panel" or "monitor", so the Game Gear
hard-coded 1.0 with a paragraph explaining why its region misled, the Game Boy
reached square through `Region::Other`, and the PET stayed on the older hook
because that hook happened to be right for it.

All three are now one line naming the kind of display.

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
is our crop, which is what #1054 is about. Over 100% means we
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

`Region` describes the signal a machine generates. It cannot stand in for the
display, and the fleet already holds the counterexamples:

- The **Game Gear** reports `Region::Ntsc`, true of its timing and silent
  about the panel it drives. `Display::Lcd`.
- The **PET** drove a 4:3 monochrome monitor and reports `Region::Other`.
  `Display::Monitor { aspect: 4.0 / 3.0 }` — and that is exact, not a
  fallback: filling 4:3 is right for a display that shows the whole raster.
- The **Amiga** running RTG would drive a monitor from a chipset that also
  feeds a set, at a clock the display card picks. `Display` can already say
  that; nothing new needs inventing when RTG lands.

## Drift triggers

Re-read this entry when you catch yourself writing:

- `Some(4.0 / 3.0)` in a new core
- an aspect ratio computed from `FB_WIDTH` / `FB_HEIGHT`
- "the framebuffer is already 4:3, so square is fine"
- a pixel clock taken from the CPU clock or from the current video mode
- an interlaced core's line count given as one field's worth
