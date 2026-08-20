# The framebuffer is the set's window

**Status:** rule stated and the fleet measured; extents span 49%–202%
horizontally and 67%–120% vertically, so most cores do not conform
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

Separating them needs each chip's rendered-against-blanked extent. Two are
now settled by inspection: the NES's 91% is the hardware, and the BBC's 77% is
the same shape — 640 dots at 16 MHz is 40 µs of a 52 µs line, and the rest is
border the core does not draw. The VIC-20's 49% is ours: the chip fills the
line, and we keep 24 border pixels a side where a set shows about 140. The
remaining cores are unclassified and #1054 tracks the pass.

## The measurement

`session.framebuffer.width` and `.height` against
`session.display.pixel_clock_hz` and `.lines_per_tv_height`, read from a
running machine through the shared headless script surface. Twenty-nine of the
thirty answer it directly. The Dragon is read from source instead, because its
frontend keeps a bespoke `--headless --cycles` harness and does not take
`--script` at all.

Nothing is parsed out of the source, and that is deliberate. The system
registry's own header records three attempts to infer its joins by pattern
matching, every one of which produced wrong answers, which is why that file
states them. Framebuffer extent needs no equivalent statement: every
`FramePacket` already carries it, so the measured value cannot drift from what
the core draws.

| Core | Framebuffer | Set's window | H | V |
|---|---|---|---|---|
| Dragon (PAL) | 744×312 | 369×288 | **202%** | 108% |
| Amiga (PAL) | 768×576 | 738×576 | 104% | 100% |
| Memotech MTX, Tatung Einstein (PAL) | 288×240 | 278×288 | 104% | **83%** |
| Atari 800XL, 5200, 7800 (NTSC) | 384×288 | 373×240 | 103% | **120%** |
| ColecoVision, MSX, Master System, SG-1000, Sord M5, SVI-328 (NTSC) | 288×240 | 280×240 | 103% | 100% |
| Acorn Atom (PAL) | 372×243 | 369×288 | 101% | 84% |
| C64 (PAL) | 416×312 | 410×288 | 101% | 108% |
| Spectrum 48K (PAL) | 352×296 | 364×288 | 97% | 103% |
| ZX80, ZX81 (PAL) | 320×288 | 338×288 | 95% | 100% |
| Jupiter Ace (PAL) | 320×240 | 338×288 | 95% | **83%** |
| Amstrad CPC (PAL) | 768×270 | 832×288 | 92% | 94% |
| NES (NTSC) | 256×240 | 280×240 | 91% | 100% |
| Mattel Aquarius (PAL) | 320×192 | 369×288 | 87% | **67%** |
| Atari 2600 (NTSC) | 160×228 | 187×240 | 86% | 95% |
| BBC Micro, Electron (PAL) | 640×256 | 832×288 | **77%** | 89% |
| Oric Atmos (PAL) | 240×224 | 312×288 | **77%** | 78% |
| VIC-20 (PAL) | 224×216 | 461×288 | **49%** | 75% |

Televisions only. The PET drives a monitor and the Game Boy and Game Gear
drive panels, so the comparison does not apply — which is `Display` doing its
job.

The range is **49%–202% horizontally and 67%–120% vertically**. #1054 opened
citing 86%–104% and 83%–108%, drawn from the seven cores that had been looked
at; the fleet is roughly three times worse than that in both directions.

## The pattern the numbers make

Six TMS9918 machines land on exactly 288×240 and 103%/100%. Two more —
Memotech MTX and Tatung Einstein — are the same chip on PAL profiles, and land
on 104%/**83%**, because 240 lines in a 288-line window is 83%. The Jupiter
Ace makes three at 83% for the same reason. It is the identical shortfall the
ZX80 had before #1053, from the identical cause: **an NTSC-shaped buffer on a
PAL machine**.

The three Atari cores are that error's mirror. `FB_HEIGHT` is
`ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM` where `ACTIVE_HEIGHT` is already
240 — the whole NTSC field — so 48 lines of border are added to a figure with
no room for them. The constant still carries a stale doc comment reading
"Framebuffer height (240 visible scan lines)" directly above the 240 it
describes.

Neither pattern is visible from one core at a time, which is the argument for
having measured all of them at once.

## Defects the audit surfaced

- **The Amiga stated no display at all.** `AmigaRuntimeKind` forwards
  `MachineCore` by hand, method by method, and `display` was not among them.
  That method has a default returning `None`, so the omission compiled and
  answered "unstated" for a machine whose three inner runtimes each state a
  television — and the harness fell back to square pixels on a machine whose
  pixels are 1.04. Fixed; `variant_dispatch.rs` covers the class rather than
  the instance.
- **The Dragon's 202% is arithmetically impossible.** 744 pixels cannot fit a
  52 µs line at 7.09 MHz; they want about 14.3 MHz, which is twice the stated
  clock. Whichever half is wrong, the pixel aspect #1053 derived for this core
  is out by a factor of two.
- **The Dragon is off the shared headless surface.** Every other frontend
  takes `--script`. This one takes `--headless --cycles` with its own flags,
  and does not document `--script` in its help even though `main.rs` names it.
  Any fleet-wide measurement has to special-case it.

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
