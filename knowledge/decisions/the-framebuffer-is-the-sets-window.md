# The framebuffer is the set's window

**Status:** rule stated and the fleet measured; the two extents that were
errors rather than choices are fixed, and the rest are classified or tracked in
#1054
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

Separating them needs each chip's rendered-against-blanked extent, and the
reference library answers it where it covers the part.

**The chip blanks it** — marked † in the table, and said so at the constant:

- The **NES** renders 256 dots of a 341-dot line and blanks the rest.
- The **Atari 2600** emits 160 colour clocks of picture in a 228-clock line;
  the other 68 are HBLANK. A set's 187-clock window holds the 160 and black.
- The **Electron**'s service manual gives it outright — the ULA is busy "40
  microseconds of each 64", and there are "312 [lines], of which 256 generate
  pixel data". 640 dots of an 832-dot window and 256 lines of 288.
- The **BBC Micro** is the same picture from its 6845: R0 gives a 128-character
  line with R1 displaying 80, and R4/R9 a 312-line frame with R6 displaying 32
  rows of 8. Neither machine has a border colour register, so what a set shows
  outside the display is black.

**We cropped it** — fixed where the border was already being drawn:

- The **Amstrad CPC** held 48 characters by 270 lines, which is Caprice32's
  `CPC_VISIBLE_SCR_WIDTH`/`HEIGHT` rather than a field. Its reference is
  explicit that the border is a 17th pen "drawn during HBLANK and overscan", so
  the machine does paint the surround and we were keeping part of it. Now 52
  characters by 288 lines, both derived from the CRTC's own totals.
- The **VIC-20** was neither. Its 49% was arithmetic on a wrong clock, and the
  extent was nearly right all along; see below.

Everything unmarked and under 100% is still unclassified, and #1054 tracks the
pass.

## The measurement

Measure every profile, not every machine. The first sweep took each frontend's
default and so saw one region per core — which hid the fact that eight machines
shared the TMS9918 family's PAL shortfall, because only two of them default to
PAL. The Atari case was the same shape from the other side. A core is not
measured until both of its regions are.

`session.framebuffer.width` and `.height` against
`session.display.pixel_clock_hz` and `.lines_per_tv_height`, read from a
running machine through the shared headless script surface. All thirty answer
it. The Dragon did not when the audit first ran — its frontend kept a bespoke
`--headless --cycles` harness and rejected `--script` — and its numbers had to
be read from source until it was given the shared surface.

Nothing is parsed out of the source, and that is deliberate. The system
registry's own header records three attempts to infer its joins by pattern
matching, every one of which produced wrong answers, which is why that file
states them. Framebuffer extent needs no equivalent statement: every
`FramePacket` already carries it, so the measured value cannot drift from what
the core draws.

| Core | Framebuffer | Set's window | H | V |
|---|---|---|---|---|
| Amiga (PAL) | 768×576 | 738×576 | 104% | 100% |
| Dragon (PAL) | 744×312 | 738×288 | 101% | 108% |
| ColecoVision, MSX, Master System, Memotech MTX, SG-1000, Sord M5, SVI-328, Tatung Einstein (PAL) | 288×288 | 278×288 | 104% | 100% |
| Atari 800XL, 5200, 7800 (PAL) | 384×288 | 369×288 | 104% | 100% |
| Atari 800XL, 5200, 7800 (NTSC) | 384×240 | 373×240 | 103% | 100% |
| ColecoVision, MSX, Master System, SG-1000, Sord M5, SVI-328 (NTSC) | 288×240 | 280×240 | 103% | 100% |
| Acorn Atom (PAL) | 372×288 | 369×288 | 101% | 100% |
| C64 (PAL) | 416×312 | 410×288 | 101% | 108% |
| Spectrum 48K (PAL) | 352×296 | 364×288 | 97% | 103% |
| ZX80, ZX81 (PAL) | 320×288 | 338×288 | 95% | 100% |
| Jupiter Ace (PAL) | 320×288 | 338×288 | 95% | 100% |
| Amstrad CPC (PAL) | 832×288 | 832×288 | 100% | 100% |
| NES (NTSC) | 256×240 | 280×240 | 91% † | 100% |
| Mattel Aquarius (PAL) | 352×224 | 369×288 | 95% | 78% ‡ |
| Atari 2600 (NTSC) | 160×240 | 187×240 | 86% † | 100% |
| BBC Micro, Electron (PAL) | 640×256 | 832×288 | 77% † | 89% † |
| Oric Atmos (PAL) | 240×224 | 312×288 | 77% † | 78% † |
| VIC-20 (PAL) | 230×288 | 231×288 | 100% | 100% |

Televisions only. The PET drives a monitor and the Game Boy and Game Gear
drive panels, so the comparison does not apply — which is `Display` doing its
job.

**†** marks a figure that is the chip rather than a crop: the core holds less
because the hardware blanks the rest, and the constant says so. **‡** marks the
one core still short of what its own chip draws — see below. Everything
unmarked and under 100% is still unclassified.

The range is **49%–104% horizontally and 67%–108% vertically**, after the
Dragon's 202% turned out to be a misstated clock rather than an extent, and the
three Atari cores' 120% a single framebuffer height serving two regions (both
below).
#1054 opened citing 86%–104% and 83%–108%, drawn from the seven cores that had
been looked at. Horizontally that upper bound held; the floor did not, and the
vertical spread is half again as wide.

## The pattern the numbers make

Six TMS9918 machines landed on exactly 288×240 and 103%/100%, and the same
chip on a PAL profile landed on 104%/**83%** — because 240 lines in a 288-line
window is 83%. The identical shortfall the ZX80 had before #1053, from the
identical cause: **an NTSC-shaped buffer on a PAL machine**.

Measuring default profiles hid how wide that was. Only the MTX and the
Einstein default to PAL, so only they appeared; the ColecoVision, MSX, Sord M5,
SVI-328, SG-1000 and Master System all had it too, on profiles the first sweep
never selected. Eight machines, one constant. `VdpRegion` and the Sega VDP's
own region now size the field, and the border is what it has left over around
the 192 lines the chip draws: 24 on NTSC, 48 on PAL. Fixed.

The Acorn Atom's 84% was a borrowed figure of a different kind. It sized its
picture from the shared VDG crate's `TEXT_VISIBLE_FRAMEBUFFER_HEIGHT`, 243 —
which is a VDG-generic "visible" figure that the Dragon places as a sub-window
inside its 312-line overscan frame. Correct for what the Dragon does with it,
and not a set's field. The Atom now states its own; the shared constant stays.

The Jupiter Ace made a ninth, from the same cause in its own video code: its
border comment said it was copying the ZX81 ULA's 24 lines "so screenshots
match the period look". #1053 then moved the ZX81 to the set's own field and
the Ace did not follow. Fixed the same way — the border is `(288 - 192) / 2`,
written as that expression rather than as 48, because the arithmetic is the
justification.

The three Atari cores were that error's mirror, at 120%. `FB_HEIGHT` was
`ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM` with `ACTIVE_HEIGHT` already 240 —
the whole NTSC field — so 48 lines of border were added to a figure that had no
room for them, and the same height then served both regions. It is correct for
PAL, which is why it survived.

GTIA and MARIA now take a region and size the field from it, with the border as
whatever is left over: nothing on NTSC, 24 lines on PAL. Both regions of all
three machines now sit at 100% vertically.

Neither pattern is visible from one core at a time, which is the argument for
having measured all of them at once.

## The VIC-20: an extent that was a clock

The audit read the VIC-20 at 49% horizontally, by far the worst figure in the
fleet, and its border comment was one of this entry's own drift triggers
verbatim — "VICE typically render ~30-40 px of border each side... a clean
approximation that matches the period look".

Widening to match would have been wrong. `PAL_PIXEL_CLOCK_HZ` was 8.867 MHz on
the reasoning that the VIC fetches one character of eight pixels per machine
cycle, and the constant checked itself against the line time — a check it
cannot fail, because doubling the pixel count and the clock together leaves the
microseconds unchanged.

VICE settles it in a comment:

```text
#define VIC_NTSC_SCREEN_WIDTH  260   /* 65 cycles * 4 pixels */
#define VIC_PAL_SCREEN_WIDTH   284   /* 71 cycles * 4 pixels */
```

Four pixels a cycle. A character spans two cycles, 22 columns occupy 44 of 71,
and the display fills three quarters of the line rather than the third the
eight implied. The published pixel aspects agree: VICE's `vic_get_pixel_aspect`
gives PAL 1.66574035 and NTSC 1.50411479 from codebase64, and these clocks
derive 1.6656 and 1.5000 where the old ones gave exactly half each.

So the horizontal extent was never 49%. At the corrected clock the old 224-wide
buffer measures 97% of a PAL window — and 224 is, to the pixel, VICE's own
`VIC_PAL_NORMAL_DISPLAY_WIDTH`. Only the height was short, at 216 of 288.

Two lessons, and the second is the one that generalises:

- **A ratio far outside the fleet's spread is a reason to check the terms, not
  to move the number.** The Dragon's 202% was the same shape, caught the same
  way.
- **A constant that checks itself against a figure both its terms cancel out of
  is not checked at all.** Both the VIC-20's clock and the eight it was derived
  from reproduce the line time exactly.

## The last two, settled by MAME

Both machines held exactly their character grid with no border at all, and
neither `reference/by-system/` folder covers what their chips put outside the
active area. MAME's driver sources do, in one line each.

**The Oric blanks it.** `mame/tangerine/oric.cpp`:

```text
screen.set_raw(12_MHz_XTAL / 2, 64*6, 0, 40*6, 312, 0, 28*8);
```

A 6 MHz dot clock; a 384-dot line whose visible window is dots 0 to 240; 312
lines whose visible window is lines 0 to 224. That visible area is *exactly*
what this project already held, and everything outside it is blanking rather
than border. So the Oric joins the BBC and the Electron: 77% and 78% are the
hardware, and the constants now say so.

**The Aquarius draws it, from screen cell 0.** `mame/mattel/aquarius_v.cpp`
builds a 44 x 29 tilemap, and `get_tile_info` gives rows 0, 1, 27, 28 and
columns 0, 1, 42, 43 the tile `m_videoram[0]` with the attribute
`m_colorram[0]`. The machine has no border colour register; the surround is
the top-left screen cell repeated, which is why writing `$3000` recolours the
whole border. `set_raw`'s 352 x 232 visible area agrees with that grid.

Adding it takes the Aquarius from 87% / 67% — the worst figure in the fleet —
to 95% / 78%, and the width is now MAME's 352 to the pixel. Border and display
go through one path, because on this machine they are the same thing.

**A discrepancy this turned up, not settled here.** MAME's grid gives 25
display rows: its border rows are 0, 1, 27 and 28, leaving rows 2 to 26. This
project draws 24, and 40 x 24 is what its own header and the reference's
"1 KB screen" describe. The one line of evidence to hand cuts the other way —
`LISTCT` in the community memory map stops LIST "after every 24 rows", which is
what a 24-row screen or a 25-row screen with a prompt line would both do. The
Aquarius's height is therefore 224 here against MAME's 232, and the ‡ in the
table marks it. Changing the row count changes what the machine displays, not
how much of it we keep, so it wants its own evidence.

## Defects the audit surfaced

- **The Amiga stated no display at all.** `AmigaRuntimeKind` forwards
  `MachineCore` by hand, method by method, and `display` was not among them.
  That method has a default returning `None`, so the omission compiled and
  answered "unstated" for a machine whose three inner runtimes each state a
  television — and the harness fell back to square pixels on a machine whose
  pixels are 1.04. Fixed; `variant_dispatch.rs` covers the class rather than
  the instance.
- **The Dragon stated half the clock its framebuffer fills at.** 744 pixels
  cannot fit a 52 µs line at 7.09 MHz. The Atom settled which half was wrong:
  same MC6847, same stated constant, 372 pixels emitted. The Dragon expands
  that picture into a 744-wide PAL overscan frame by writing every pixel twice
  — a plain doubling that carries no extra detail, done to give the overscan
  frame roughly square pixels. Its pixels were therefore derived twice as wide
  as the ones it emits. Fixed by stating the rate the framebuffer fills at;
  the pixel aspect moves from 1.041 to 0.5205 and the extent from 202% to
  101%.
- **The Dragon was off the shared headless surface.** Every other frontend
  took `--script`; this one grew a harness of its own first and never gained
  it, so `main.rs` routed the flag to a parser that rejected it. That put the
  machine outside everything built on the common query paths, and meant the
  audit had to special-case it — reading numbers from source, which is exactly
  the inference this project avoids elsewhere. Fixed; the Dragon's row above
  is now measured like the rest.

## What blocks the rest of the fleet

**Goldens.** Changing a framebuffer's dimensions invalidates every frame hash
and screenshot taken against it. The ZX80 and ZX81 could move because neither
has committed image goldens. The catalogue systems cannot move so cheaply.

That is now the only blocker. The instrument exists.

## Drift triggers

Re-read this entry when you catch yourself writing:

- a framebuffer size chosen to fit the picture snugly
- "the extra is only border, so cropping it is free"
- a border constant picked to look right in a screenshot
- an extent audit that reads under-100% as a crop without checking what the
  chip blanks
- a 240-line buffer on a PAL profile, or a 288-line buffer on an NTSC one
- a framebuffer constant written `ACTIVE + BORDER + BORDER` without checking
  the sum against the set's window
- a new hand-written `impl MachineCore` — every trait method that has a
  default fails silently rather than at the compiler
- an extent measured from a frontend's default profile, which sees one region
  and calls the core done
- a ratio far outside the fleet's spread treated as a number to move rather
  than terms to check — twice now it has been the pixel clock
- a constant justified by a check both its terms cancel out of, such as pixels
  a cycle against the line time
