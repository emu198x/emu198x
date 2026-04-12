# Amiga Graphics and Display Subsystem Reference

**Scope.** This document describes how the Amiga *actually produces pixels*, from Agnus
fetching bitplane words out of Chip RAM all the way up to `intuition.library` routing
an IDCMP message when you click in a window. It is written for people building a
hardware-accurate emulator and therefore favours bit-level and cycle-level detail over
user-facing API narration.

**Companion documents.**
- `/Users/stevehill/Desktop/AmigaPDFs/amiga-boot-process.md` covers the power-on and
  Kickstart boot sequence. This document does **not** repeat the boot path; any mention
  of "after reset" defers to that file.
- `/Users/stevehill/Desktop/AmigaPDFs/amiga-hardware-reference.md` (may not yet exist)
  is the register-by-register datasheet. This document assumes you can look up register
  offsets and bit names there, and focuses on *how the registers interact* across a
  frame.

**Source corpus.** All material is drawn from ten extracted PDFs in
`/Users/stevehill/Desktop/AmigaPDFs/txt/`:

| Short form   | File |
|--------------|------|
| HRM          | `Amiga_Hardware_Reference_Manual_3rd_edition.txt` |
| RKM L&D      | `Amiga_ROM_Kernel_Reference_Manual_Libraries_and_Devices.txt` |
| RKM 3rd      | `1990-beats-steve-amiga-rom-kernel-ref-3rd.txt` |
| Autodocs     | `Amiga_ROM_Kernal_Reference_Manual_Includes_and_Autodocs.txt` |
| SPG          | `Amiga_System_Programmers_Guide_1988_Abacus.txt` |
| Abacus ML    | `Amiga_Machine_Language_1991_Abacus.txt` |
| Mapping      | `1993-thomson-randy-rhett-anderson-mapping-amiga-2nd-edition.txt` |
| A500 TRM     | `Commodore_Amiga_A500_A2000_Technical_Reference_Manual_1987_Commodore_text.txt` |
| Exec RKM     | `Amiga_ROM_Kernel_Reference_Manual_Exec.txt` |

Citations are inline, e.g. `(HRM §Copper p.22)`.

**How to read this document.** Start with Section 1 if you want to understand the
frame timing; it is the spine everything else hangs off. Blitter and Copper sections
are self-contained and can be read in any order. The function indices at the end are
lookup tables, not reading matter.

---

## Table of Contents

1. [The display pipeline](#1-the-display-pipeline)
2. [Playfield modes](#2-playfield-modes)
3. [Sprites](#3-sprites)
4. [The Copper](#4-the-copper)
5. [The Blitter](#5-the-blitter)
6. [Color and palette](#6-color-and-palette)
7. [graphics.library](#7-graphicslibrary)
8. [intuition.library](#8-intuitionlibrary)
9. [layers.library](#9-layerslibrary)
10. [Tying it together](#10-tying-it-together)
11. [Display timing reference](#11-display-timing-reference)
- [Appendix A — Copper instruction encoding](#appendix-a--copper-instruction-encoding)
- [Appendix B — BLTCON0 / BLTCON1 bit tables](#appendix-b--bltcon0--bltcon1-bit-tables)
- [Appendix C — Color register map](#appendix-c--color-register-map)
- [Appendix D — graphics.library function index](#appendix-d--graphicslibrary-function-index)
- [Appendix E — intuition.library function index](#appendix-e--intuitionlibrary-function-index)
- [Gaps in corpus](#gaps-in-corpus)
- [Source map](#source-map)

---

## 1. The display pipeline

### 1.1 What Agnus, Denise and Paula each do

Agnus is the DMA controller, timing generator, Copper host, and Blitter host. It owns
the memory bus to Chip RAM and decides which device gets which slot. Denise is the
video output chip: it serialises bitplane words, runs the playfield→colour lookup,
serialises sprite data, mixes sprites and playfield according to priority, and emits
RGB. Paula handles audio, disk, serial, mouse/joystick counters, and interrupts, and
does *not* participate in the pixel path except that it latches the light-pen position
into VPOSR/VHPOSR and reads the controller ports that Intuition uses for mouse input
(HRM §Interface Hardware p.227). For graphics, you can ignore Paula except for
interrupts: Paula raises VERTB/COPER/BLIT/PORTS on Agnus's behalf through its
INTENA/INTREQ logic.

The frame is driven by the master oscillator — *not* by the CPU, and not by any
software loop. On NTSC the crystal is 28.37516 MHz (PAL same numeric, slightly
different; see HRM §Appendix p.19915). Divide by 4 to get the 7.15909 MHz "7M" CPU
clock, and by 8 to get the 3.579545 MHz colour-clock rate
(HRM §Coprocessor Hardware p.1478). *One horizontal line on NTSC is 227.5 colour
clocks long* (HRM §Blitter Hardware p.8868). Everything in the display is indexed in
colour clocks by Agnus's horizontal counter, and in lines by Agnus's vertical counter.
Those counters are what the Copper WAITs against and what Denise clocks pixels from.

Because one colour clock = one 140 ns memory cycle = two 70 ns hires pixels, the
natural units of the display are:

- **1 colour clock** = 1 memory slot, 2 lores pixels, 4 hires pixels.
- **NTSC line** = 227.5 colour clocks = ~63.56 µs. Alternating lines are 227 or 228
  colour clocks long to make the half-line average work. PAL: all lines are 227, the
  half-clock is elsewhere (HRM p.1434: *"All lines are not the same length in NTSC.
  Every other line is a long line (228 color clocks, 0-$E3), with the others being
  227 color clocks long. In PAL, they are all 227 long."*).
- **NTSC frame** = 262 lines (short) or 263 lines (long) alternating in interlace,
  totalling a 4-field pattern
  (HRM p.1467: *"In NTSC, the fields are 262, then 263 lines and in PAL, 312, then 313
  lines."*). Non-interlaced NTSC loops at ~262.5 lines per field.
- **PAL frame** = 312 or 313 lines per field. Visible lines ~256 non-interlaced,
  ~512 interlaced.

The hardware vertical blank interval minimum is 20 lines NTSC, 25 lines PAL, starting
at line 0 (HRM §System Control p.9920).

### 1.2 Bitplane DMA: from BPLxPT to Denise shift register

The basic loop (HRM Chapter 3, passim):

1. Agnus has 6 bitplane pointers, `BPL1PTH/BPL1PTL`..`BPL6PTH/BPL6PTL`
   (register offsets $0E0..$0EA). These are *dynamic* — Agnus increments them by two
   for every word fetched.
2. At the start of each displayed line, Agnus uses the horizontal counter to decide
   when bitplane DMA starts: it compares against `DDFSTRT` ($092). Only bits H8..H3
   are compared (HRM p.12319: *"USE X X X X X X X X H8 H7 H6 H5 H4 H3 X X"*), so
   `DDFSTRT` has a resolution of 8 lores pixels (4 hires).
3. For each fetch slot inside the range [DDFSTRT, DDFSTOP], Agnus issues a DMA
   read for the active bitplane, round-robin across the 1–6 enabled bitplanes. The
   word read from Chip RAM is latched into `BPLxDAT` ($110 + 2·x).
4. Writing `BPL1DAT` triggers the parallel-to-serial converters in Denise. HRM p.12062
   is explicit: *"The parallel-to-serial conversion is triggered whenever bitplane #1
   is written, indicating the completion of all bitplanes for that word (16 pixels).
   The MSB is output first, and is, therefore, always on the left."* This is why
   bitplane 1 must always be written last within a slot group, and it is why the
   hardware gracefully handles a 1-bitplane display (every fetch loads BPL1DAT).
5. Denise shifts each `BPLxDAT` one bit per lores pixel (two bits per hires pixel),
   combining one bit from each active plane into a 1..6-bit colour index per pixel.
6. That index feeds the colour-lookup stage (§6 below) which produces 12-bit RGB.
7. At end-of-line, Agnus adds `BPL1MOD` ($108) to the odd-numbered bitplane pointers
   and `BPL2MOD` ($10A) to the even-numbered ones. Modulos are *byte* values. For a
   single full-width playfield the modulo is 0 (the next line in memory is exactly
   after the last word of the current line); for a wider-than-display bitmap the
   modulo is the extra bytes to skip (HRM p.3023 onwards with detailed diagrams).

Because DDFSTRT compare only respects H8..H3, *the smallest resolvable start
position is 8 lores pixels / 16 lores-pixel-width bitplane words… in practice HRM
recommends restricting DDFSTRT to 16-pixel granularity (8 colour clocks in lores,
4 in hires) because "the hardware requires some time after the first data fetch
before it can actually display the data"* (HRM p.2988).

### 1.3 The two windows: data fetch versus display

The Amiga has two independent windowing stages, and confusing them is the most common
source of bugs in new Amiga code (and emulators).

**DDFSTRT/DDFSTOP** define *which memory slots* Agnus uses to fetch bitplane data.
They are in colour clocks (though only H8..H3 are meaningful). Once a fetch sequence
starts at DDFSTRT, it issues one fetch per word position and stops when the horizontal
counter reaches DDFSTOP.

**DIWSTRT/DIWSTOP** define *the rectangular area of the beam during which Denise is
allowed to emit playfield pixels*. DIWSTRT/DIWSTOP have one-pixel resolution (HRM
p.2983: *"The data-fetch registers have a four-pixel resolution (unlike the display
window registers, which have a one-pixel resolution)"*). Their units are **always
lores non-interlaced** regardless of the actual mode (HRM p.2909: *"The starting
position is always interpreted in low resolution, non-interlaced mode."*).

The two windows must be aligned carefully:

> "The hardware requires some time after the first data fetch before it can actually
> display the data. As a result, there is a difference between the value of window
> start and data-fetch start of 4.5 color clocks. The normal low resolution DDFSTRT is
> ($0038). The normal high resolution DDFSTRT is ($003C)." (HRM p.2991)

The math works like this: `DIWSTRT.HSTART` is in lores-pixel coordinates, so halve it
to get colour clocks, then subtract the 4.5-colour-clock Denise latency:

```
lores:  DDFSTRT = (HSTART/2) - 8.5   → $81/2 - 8.5 = $38
hires:  DDFSTRT = (HSTART/2) - 4.5   → $81/2 - 4.5 = $3C
```

(HRM p.2998 shows exactly this calculation.)

`DDFSTOP` is related to DDFSTRT by the number of words you want to fetch:

```
lores:  DDFSTRT = DDFSTOP − 8 × (wordcount − 1)
hires:  DDFSTRT = DDFSTOP − 4 × (wordcount − 2)
```

(HRM p.3006 verbatim.)

Nominal values for a standard NTSC 320×200 lores non-interlaced screen:

```
DIWSTRT = $2C81   ; VSTART=$2C, HSTART=$81
DIWSTOP = $F4C1   ; VSTOP=$F4, HSTOP=$C1 (actual $1C1; top bit implicit)
DDFSTRT = $0038
DDFSTOP = $00D0
```

(HRM p.3243–3246; Playfield Hardware sample code.)

A few oddities that an emulator must reproduce:

- **HSTOP is stored −256.** HRM p.2936: *"Note that the HSTOP value you write is the
  actual value minus 256 ($100)."* Hardware forces the MSB of HSTOP to be the
  complement of the next MSB, giving a range of 256..511. Same trick for VSTOP:
  *"VSTOP is restricted to the lower half of the screen. This is accomplished in the
  hardware by forcing the MSB of the stop position to be the complement of the next
  MSB."* (HRM p.2941).
- **HRM limits for OCS.** DDFSTRT ≥ $18, DDFSTOP ≤ $D8. Max 25 words fetched lores
  (49 words hires) but horizontal blanking limits the *displayable* video to 368
  lores pixels (23 words). (HRM p.3893, Table 3-14).
- **DDFSTRT < $38 disables sprite 7** (HRM p.4021 and p.5561), because bitplane DMA
  steals the slot that would have been sprite 7's.
- **Enhanced Chip Set (ECS) extends the window registers.** There is a `DIWHIGH`
  register ($1E4) that provides upper bits for DIWSTRT/DIWSTOP (HRM p.12345). On OCS
  DIWHIGH is unimplemented and the old scheme applies (HRM p.13700).

### 1.4 Fetch vs display: a worked example

For a 320-lores screen with bitplane data at $21000 (even Chip RAM address):

```assembly
MOVE.W  #$1200,BPLCON0(a0)   ; 1 bitplane, color burst on
MOVE.W  #$0000,BPLCON1(a0)   ; zero scroll
MOVE.W  #0,BPL1MOD(a0)       ; modulo zero (bitmap == display)
MOVE.W  #$0038,DDFSTRT(a0)
MOVE.W  #$00D0,DDFSTOP(a0)
MOVE.W  #$2C81,DIWSTRT(a0)
MOVE.W  #$F4C1,DIWSTOP(a0)
MOVE.L  #$00021000,BPL1PTH(a0)  ; long write goes to H then L
```

(adapted from HRM p.3239 onwards.)

Trace for line N inside the visible range:

1. At horizontal colour-clock $38, Agnus starts fetching. It reads word 0 of
   bitplane 1 from $21000, latches it into BPL1DAT, which arms the serialiser.
2. Five colour clocks later (at about $45), Denise begins emitting pixel 0.
3. Fetches continue every 8 colour clocks (one slot per word) until $D0.
4. That yields 20 word fetches (($D0−$38)/8 + 1 = 20), covering 320 lores pixels.
5. At end of line, BPL1PT has advanced to $21000 + 40; BPL1MOD (=0) is added, so
   BPL1PT for line N+1 is still $21028 = $21000 + 40. Next line starts at $21028.
6. At end of frame (line F4), vertical blank triggers. The graphics VBlank server
   writes COP1LC back to the start of the Copper list, and the Copper's first job
   in its list is to re-load BPL1PTH/BPL1PTL back to $21000.

### 1.5 Scrolling (BPLCON1)

Horizontal scrolling uses the same bitplane data and the same DDF window; it just
delays the pixel output inside Denise. `BPLCON1` bits 7–0:

```
BPLCON1 bits 3-0  PF1H(3-0)  — playfield 1 delay, 0..15 lores pixels
BPLCON1 bits 7-4  PF2H(3-0)  — playfield 2 delay, 0..15 lores pixels
```

(HRM p.4420.)

In single-playfield mode you must load both nibbles with the same value (HRM p.4139:
*"Warning: Always set all six bits, even if you have only one playfield. Set 3-0 and
7-4 to the same value if you are using only one playfield."*). To scroll, you also:

- Start DDFSTRT one word earlier than the non-scrolled position (HRM p.4020:
  *"The normal data-fetch start for non-scrolled displays is ($38). If horizontal
  scrolling is desired, then the data fetch must start one word sooner (DDFSTRT =
  $0030). Incidentally, this will disable sprite 7."*).
- Set the modulo to (picture-width − display-width − 2 bytes) because you fetched
  one extra word per line.
- Advance/retreat BPLxPT across frames by whole-word increments to get coarse
  scroll; the delay in BPLCON1 gives the fine scroll.

Vertical scrolling is simpler: increment/decrement BPLxPT by a multiple of the line
stride and let the modulo take care of the rest (HRM p.3944).

### 1.6 Overscan

The beam covers more area than is normally visible: hardware HSTART limits are $18..$D8
(in DDF coords) and VSTART limits are 0..$17F. The DIWSTRT/DIWSTOP ranges allow
going substantially beyond the default "centred 320×200" window to produce overscan
displays, at the cost of disabling some sprites (bitplane DMA steals their slots —
HRM p.5562). The ECS DIWHIGH register ($1E4) pushes the coordinate space even
further.

### 1.7 Interlace

In interlace, successive *fields* offset vertically by half a scanline — HRM p.2559.
The hardware alternates between a "long frame" (263 NTSC / 313 PAL lines) and a
"short frame" (262/312 lines) to make interlace work; the `LOF` bit (bit 15 of
VPOSR) tells you which one you're in:

> "LOF (Long-frame bit). Used to initialize interlaced displays." (HRM p.9779)

For interlaced bitplanes you use a bitmap twice as tall and `BPL1MOD/BPL2MOD = 40`
(one line worth) so each field fetches alternating lines (HRM p.3102). The even
field starts at `BPLxPT = base`, the odd field starts at `BPLxPT = base + 40`. A
VBlank interrupt or Copper list rewrite handles the pointer swap (HRM p.3333). The
graphics library's `LoadView()` handles this automatically with LOFCprList and
SHFCprList.

### 1.8 The beam counter, VPOSR and VHPOSR

The beam counter is readable at `VPOSR` ($004) and `VHPOSR` ($006). VPOSR upper
byte is status (LOF, ECS chip ID) and its low bit is V8 (the 9th bit of vertical
position, needed to distinguish PAL line 312 from NTSC line 256). VHPOSR is
`V7..V0 | H8..H1`. Horizontal resolution is thus 1/160th of the screen width
(HRM p.9787). The Copper's WAIT instruction checks these same counters.

---

## 2. Playfield modes

### 2.1 BPLCON0 bit layout

BPLCON0 ($100) controls mode, depth, and various flags. The whole register **cannot
be set one bit at a time** — every write replaces all 16 bits, so you always rebuild
the word. HRM p.4366 verbatim:

```
Bit  Name       Meaning
15   HIRES      1 = 640-pixel hires mode, 0 = 320-pixel lores
14   BPU2       ┐
13   BPU1       ├ bitplanes used (0..6)
12   BPU0       ┘
11   HOMOD      1 = Hold-and-modify; 0 = EHB enabled if DBLPF=0 & BPU=6
10   DBLPF      1 = Dual playfield; 0 = single playfield
 9   COLOR      1 = color burst enabled (composite only)
 8   GAUD       Genlock audio enable (muxed on BKGND pin during blanking)
 7-4 (unused)
 3   LPEN       Light pen enable
 2   LACE       Interlace enable
 1   ERSY       External resync (HSYNC/VSYNC become inputs — genlock)
 0   (unused)
```

Number of bitplanes (HRM Table 3-5):

```
BPU2..0   Planes active
000       None — background color only
001       PLANE 1
010       PLANES 1-2
011       PLANES 1-3
100       PLANES 1-4
101       PLANES 1-5
110       PLANES 1-6    ← only in dual-playfield or HAM
111       unused
```

BPU=110 (6 bitplanes) is **only valid** in HAM (single playfield, low-res) or in dual
playfield (3 planes per field), or in EHB mode with HOMOD=0 (HRM p.4648). Setting 6
planes in hires is invalid.

### 2.2 Low-res vs high-res vs super-hires

**Low-res** (HIRES=0): 320 pixels per line, 1 colour clock = 2 pixels. Up to 6
bitplanes (with HAM/EHB/dual-pf limits). DDFSTRT and DDFSTOP fetch one word per 8
colour clocks.

**High-res** (HIRES=1): 640 pixels, 1 colour clock = 4 pixels, 2 pixels per
70 ns "hires pixel time". Up to **4 bitplanes** — 6-plane hires is impossible on
OCS/ECS because bitplane DMA would need more than the available slots (HRM p.9027:
*"If you specify four high resolution bitplanes (640 pixels wide), bitplane DMA needs
all of the available memory time slots during the display time just to fetch the 40
data words for each line of the four bitplanes (40 * 4 = 160 time slots). This
effectively locks out the 68000 (as well as the blitter or Copper)."*).

**Super-hires** (ECS only): 1280 pixels. Not covered in detail by HRM (appendix C
references). See §Gaps below.

Interlace (LACE=1) doubles vertical resolution: 400 NTSC / 512 PAL (non-interlace
200/256, HRM p.2553). It is orthogonal to HIRES.

### 2.3 Color selection by mode

HRM Tables 3-17/3-18/3-19 (lightly cleaned):

**Low-res, single playfield, normal:**
```
5 bits from planes 5,4,3,2,1 select COLOR00..COLOR31
(plane 5 = MSB of the 5-bit color index)
```

**Low-res, single playfield, HAM:**
```
Bit6 Bit5  Result
 0    0    Normal — planes 4..1 select COLOR00..COLOR15
 0    1    Hold green, red; B = bits from planes 4..1
 1    0    Hold green, blue; R = bits from planes 4..1
 1    1    Hold blue, red;  G = bits from planes 4..1
```

At the start of each scan line, HAM begins with the background colour (COLOR00),
*not* with a carry from the previous line (RKM L&D p.2182). Bits 6/5 in HAM pick
which channel to modify, and bits 4..1 are the 4-bit new value for that channel.

**Low-res, single playfield, EHB (Extra-Halfbrite):** triggered by BPU=6, DBLPF=0,
HOMOD=0. Bitplane 6 controls a half-intensity shift of the color selected by planes
1..5 — so you effectively get 32 normal colours plus 32 half-intensity versions, for
64 simultaneous (HRM p.4650). The "halfbrite" operation is "colour register contents
shifted right by 1 in each gun".

**Hires, single playfield:**
```
4 bits from planes 4,3,2,1 select COLOR00..COLOR15
```

**Dual playfield (DBLPF=1):**
- Playfield 1 (PF1) uses odd-numbered planes: 1, 3, 5. Up to 3 bits →
  COLOR01..COLOR07 (COLOR00 = transparent).
- Playfield 2 (PF2) uses even-numbered planes: 2, 4, 6. Up to 3 bits →
  COLOR09..COLOR15 (COLOR08 = transparent).
- In hires DPF, only 2 planes per playfield (COLOR01..03 / COLOR09..11).
- Scrolling (BPLCON1) is separate for each playfield via PF1H/PF2H nibbles.
- Relative priority is controlled by `PF2PRI` in BPLCON2 bit 6.

> "When PF2PRI = 1, playfield 2 has priority over playfield 1. When PF2PRI = 0,
> playfield 1 has priority." (HRM p.3531)

COLOR00 always shows as background under everything. In dual playfield, colour 000
in *either* playfield means transparent — you see through that playfield to whatever
is behind (HRM p.3484). Transparent windows must be used carefully with independent
scrolling (HRM p.3548: *"If you want to scroll one playfield and hold the other, you
must adjust the data-fetch start and data-fetch stop to handle the playfield being
scrolled. Then, you must adjust the modulo and the bitplane pointers of the playfield
that is not being scrolled to maintain its position on the display."*).

### 2.4 Colour clocks per pixel

This is the table you need to implement the display rasteriser accurately:

| Mode         | Pixels per colour clock | Lores-pixel units per colour clock |
|--------------|-------------------------|------------------------------------|
| LORES        | 2                       | 2                                  |
| HIRES        | 4                       | 2                                  |
| SHRES (ECS)  | 8                       | 2                                  |

Note that DIWSTRT/DIWSTOP and sprite positions are *always* in lores non-interlaced
units regardless of mode (HRM p.2910), so in hires you see each "HSTART position"
cover 2 hires pixels.

### 2.5 Double buffering

RKM L&D p.2065 lists the recipe:

1. Allocate two BitMaps.
2. Allocate one ViewPort.
3. Set the RasInfo.BitMap to one of them.
4. Call `MakeVPort()` to generate Copper lists.
5. To flip: rewrite the bitplane-pointer MOVE instructions in the Copper list to
   point at the other BitMap, then `MrgCop()` / `LoadView()`.

Hardware-side, all the flip does is swap the six BPLxPT values at VBlank. The
graphics library does this with dynamically-created Copper lists embedded in the
ViewPort (LOFCprList / SHFCprList for long frame / short frame in interlace).

---

## 3. Sprites

### 3.1 The 8 sprite DMA channels

There are 8 sprite channels. Each has:

- A pointer pair `SPRxPTH/SPRxPTL` ($120 + 4·x), loaded into BPL/sprite slots by
  the system during vertical blank (or by the Copper).
- A position register `SPRxPOS` ($140 + 8·x), containing `VSTART[7..0]` in bits 15-8
  and `HSTART[8..1]` in bits 7-0.
- A control register `SPRxCTL` ($142 + 8·x), containing `VSTOP[7..0]` in bits 15-8,
  `ATT` (attach) in bit 7, and the high bits `SV8`, `EV8`, `SH0` in bits 2-0 (HRM
  p.12741).
- Two data registers `SPRxDATA` ($144+8·x) and `SPRxDATB` ($146+8·x) holding the
  current 16-pixel line of the sprite.

Each pair of channels shares a group of 4 colour registers:
- Sprites 0,1 → COLOR17–19 (with 16 unused)
- Sprites 2,3 → COLOR21–23 (with 20 unused)
- Sprites 4,5 → COLOR25–27 (with 24 unused)
- Sprites 6,7 → COLOR29–31 (with 28 unused)

The "unused" colour register in each group corresponds to pixel value 00 (transparent)
and its RGB value is ignored (HRM p.4930).

### 3.2 Sprite data stream format

A sprite data block is (HRM Table 4-1):

```
word 0: SPRxPOS value   (VSTART_lo, HSTART_hi)
word 1: SPRxCTL value   (VSTOP_lo, ATT, ..., SV8, EV8, SH0)
word 2: data line 1 low-plane  (bits that become color-bit-0 per pixel)
word 3: data line 1 high-plane (bits that become color-bit-1 per pixel)
word 4: data line 2 low-plane
word 5: data line 2 high-plane
 ...
word 2n: 0x0000   (end of sprite — next POS word is all zero)
word 2n+1: 0x0000 (ditto; or could be a new POS/CTL pair for sprite reuse)
```

All sprite data must be word-aligned in Chip RAM (HRM p.5041).

When sprite DMA is on, Agnus fetches SPRxPOS and SPRxCTL during the horizontal blank
following VSTART, loading them into Denise's position comparators. On each subsequent
line the sprite DMA engine fetches SPRxDATA and SPRxDATB for that line. **Writing
SPRxDATA arms the sprite**, so the order of writes matters — SPRxDATB first, then
SPRxDATA (HRM p.5899). The two words are shifted out in parallel, one pair of bits
per lores pixel, producing a 2-bit colour index which indexes the sprite's 4-colour
group.

### 3.3 Attached sprites (15 colours)

Setting bit 7 (`ATT`) in SPRxCTL of an **odd-numbered** sprite (1, 3, 5, or 7) pairs
it with the even-numbered sprite below it. Together they provide a 4-bit colour
index which indexes all 16 registers in the adjacent group (HRM p.5832). Colour 0000
is transparent.

> "The highest numbered sprite (number 1, in this example) contributes the highest
> order bits (leftmost) in the binary number. The high order data word in each
> sprite contributes the leftmost digit." (HRM p.5812)

### 3.4 Sprite priority

Sprite 0 is always in front of sprite 1, which is always in front of sprite 2, and
so on. You can't change it (HRM p.9470). What you *can* control is where the
playfields sit in that priority stack using BPLCON2 bits 5-0:

```
BPLCON2 bits 2-0  PF1P[2..0]  Playfield 1 position in sprite stack
BPLCON2 bits 5-3  PF2P[2..0]  Playfield 2 position in sprite stack
BPLCON2 bit  6    PF2PRI      1 = PF2 in front of PF1
```

(HRM p.9541.)

Table 7-2 for PF1:
```
000  PF1  SP01 SP23 SP45 SP67   (PF in front of all sprites)
001  SP01 PF1  SP23 SP45 SP67
010  SP01 SP23 PF1  SP45 SP67
011  SP01 SP23 SP45 PF1  SP67
100  SP01 SP23 SP45 SP67 PF1   (PF behind all sprites)
```

(HRM p.9566.)

### 3.5 Sprite reuse via the Copper

Each sprite DMA channel can be reused multiple times per field (HRM p.5599). The
end-of-sprite marker is two zero words — but if those are followed by a non-zero
POS word for a later line, the sprite channel simply re-arms and displays again.
Alternatively, you can rewrite SPRxPTH/SPRxPTL from the Copper between usages.

The restriction is that **one blank scan line must separate usages** (HRM p.5665)
because each sprite gets only two DMA cycles per line and needs at least one of
them for the new POS/CTL fetch.

### 3.6 Sprites and bitplane DMA contention

Sprites use 16 of the 226 memory cycles per line (2 words × 8 sprites), in sprite
time slots between the audio and bitplane slots. If you start bitplane DMA earlier
(`DDFSTRT < $38`), bitplane DMA steals sprite DMA slots starting from sprite 7 and
working down (HRM Figure 6-9). An emulator should model this as a DMA priority list
per slot.

### 3.7 Collision detection

Collision bits are registered in `CLXDAT` ($00E, read-only, cleared on read),
controlled by `CLXCON` ($098). The CLXDAT bits (HRM p.9671):

```
Bit Collision
14  Sprite 4/5 ↔ sprite 6/7
13  Sprite 2/3 ↔ sprite 6/7
12  Sprite 2/3 ↔ sprite 4/5
11  Sprite 0/1 ↔ sprite 6/7
10  Sprite 0/1 ↔ sprite 4/5
 9  Sprite 0/1 ↔ sprite 2/3
 8  Even bitplanes ↔ sprite 6/7
 7  Even bitplanes ↔ sprite 4/5
 6  Even bitplanes ↔ sprite 2/3
 5  Even bitplanes ↔ sprite 0/1
 4  Odd bitplanes ↔ sprite 6/7
 3  Odd bitplanes ↔ sprite 4/5
 2  Odd bitplanes ↔ sprite 2/3
 1  Odd bitplanes ↔ sprite 0/1
 0  Even bitplanes ↔ odd bitplanes
```

CLXCON (HRM p.9713) lets you include/exclude the odd sprites (bit 15-12: ENSP7,
ENSP5, ENSP3, ENSP1) and define match conditions for each bitplane (bits 11-6 =
include bitplanes in match, bits 5-0 = required logical state of those bitplanes).

Collisions fire whenever two *enabled* objects are non-zero at the same pixel.
Because they test the raw bitplane bits, not the final colour, collision detection
works independently of single vs. dual playfield mode (HRM p.9699).

### 3.8 Manual mode

You can bypass sprite DMA entirely and write `SPRxPOS`, `SPRxCTL`, `SPRxDATB` then
`SPRxDATA` directly from the CPU or Copper. Sprites activated manually display as
horizontal repeats on every line until disarmed by a write to SPRxCTL
(HRM p.5891–5906).

---

## 4. The Copper

### 4.1 What the Copper is

The Copper is a tiny DMA-based coprocessor inside Agnus. Its program counter is
loaded from COP1LC on VBlank, and it fetches one 32-bit instruction at a time from
Chip RAM — all its fetches go through the normal Agnus DMA slot mechanism, so the
Copper only uses *odd-numbered* colour-clock slots (HRM p.1258: *"The Copper is a
two-cycle processor that requests the bus only during odd-numbered memory cycles.
This prevents collision with audio, disk, refresh, sprites, and most low resolution
display DMA access, all of which use only the even-numbered memory cycles."*).

The Copper has **three instructions**: MOVE, WAIT, SKIP. Each is exactly 32 bits
(two words).

- MOVE uses **2 DMA cycles** (2 instruction words) but consumes **4 colour-clock
  times** because the Copper alternates even/odd (HRM p.1283).
- WAIT uses **3 DMA cycles** and takes **6 colour-clock times** — *"one extra
  memory cycle to wake up"*.
- SKIP has the same DMA profile as MOVE but the *next* instruction may or may not
  execute.

Two location registers, `COP1LCH/COP1LCL` ($080/$082) and `COP2LCH/COP2LCL`
($084/$086), each 18 bits wide (19 on ECS with more chip RAM). Two strobe
registers, `COPJMP1` ($088) and `COPJMP2` ($08A), reload the Copper PC from COP1LC
or COP2LC respectively (HRM p.1551).

**At the start of each vertical blank, Agnus automatically reloads the Copper PC
from COP1LC** — that is the fundamental looping mechanism. No software instruction is
required; it happens every frame (HRM p.1532). The Copper then runs its entire list
and stalls on whatever final WAIT is impossible-to-reach, until the next VBlank.

### 4.2 MOVE instruction

```
IR1 (first word):
  Bits 15-9  must be zero (reserved)
  Bits  8-1  DA[8..1]  Register destination address (divided by 2)
  Bit     0  = 0       MOVE marker

IR2 (second word):
  Bits 15-0  RD[15..0] Data to move
```

(HRM Table 2-2 p.2039.)

So, to move $0002 into `BPL1PTH` at $0E0, you emit `DC.W $00E0,$0002`. The
destination is a chip register address relative to $DFF000; the low bit 0 of the
first word says "I'm a MOVE".

Which registers the Copper can reach:

- `$080..$1FE` inclusive: always writable.
- `$040..$07E` (blitter control and data): writable only if `CDANG` (bit 1 of COPCON
  $02E) is set. **CDANG is cleared on reset** (HRM p.1573). The system sets it
  temporarily when graphics needs to issue blits from the Copper.
- `$000..$03E`: never writable by the Copper.

Writing to COPJMP1/COPJMP2 *from a Copper MOVE* is the mechanism by which the Copper
jumps to itself — see the Copper-loop example below.

### 4.3 WAIT instruction

```
IR1 (first word):
  Bits 15-8  VP[7..0]   Vertical beam position to wait for
  Bits  7-1  HP[8..2]   Horizontal beam position to wait for (H1 not used in compare)
  Bit     0  = 1        WAIT/SKIP marker

IR2 (second word):
  Bit    15  BFD        Blitter-finished-disable
                        (1 = normal beam-only wait;
                         0 = wait until beam match AND blitter finished)
  Bits 14-8  VE[6..0]   Vertical enable mask
  Bits  7-1  HE[6..0]   Horizontal enable mask
  Bit     0  = 0        WAIT marker
```

(HRM p.1378 and p.2039.)

The compare logic is: `((actual_beam XOR IR1) AND IR2[14..1]) == 0`, for the beam
to match. The compare is for *greater-than-or-equal*, so if you pass the target the
WAIT immediately falls through (HRM p.1417: *"the comparison operation is waiting
for the beam position to become greater than or equal to the entered position"*).

The blitter-finished bit, if 0, makes the WAIT additionally require the blitter to
be idle — this is how you force a Copper to not start a new blit until the previous
one has finished.

**VP is only 8 bits**, so positions 256..262 on NTSC (256..312 on PAL) are expressed
by waiting for `VP=$FF, HP=0` (which the counter reaches at line 255), then issuing
a second WAIT with VP in [0,5] and any HP — the Copper will drop through at line 256
when the vertical wraps (HRM p.1450). The hardware counter actually has a 9th bit,
V8, visible in VPOSR, but the Copper's WAIT comparison doesn't see it.

**HP ranges $00..$E2**, *with H1 not compared*, so 113 positions exist (4-pixel
resolution lores, 8-pixel hires — HRM p.1428).

The "impossible wait" `$FFFF, $FFFE` means VP=$FF, HP=$FE (max HP ≤ $E2, so never
reached). This is the conventional end-of-list marker (HRM p.1401: *"Wait for line
255, H = 254 (never happens)"*).

### 4.4 SKIP instruction

```
IR1: as for WAIT (bit 0 = 1)
IR2:
  Bit    15  BFD
  Bits 14-1  VE/HE as WAIT
  Bit     0  = 1      SKIP marker
```

SKIP causes the Copper to skip the *following instruction* if the beam has already
passed the (VP,HP) target, otherwise the next instruction executes normally. This is
how you build conditional jumps (HRM p.1801).

### 4.5 Instruction summary table (HRM Table 2-2 p.2039)

```
          MOVE            WAIT            SKIP
Bit    IR1     IR2     IR1    IR2     IR1    IR2
15     X       RD15    VP7    BFD     VP7    BFD
14     X       RD14    VP6    VE6     VP6    VE6
13     X       RD13    VP5    VE5     VP5    VE5
12     X       RD12    VP4    VE4     VP4    VE4
11     X       RD11    VP3    VE3     VP3    VE3
10     X       RD10    VP2    VE2     VP2    VE2
09     X       RD09    VP1    VE1     VP1    VE1
08     DA8     RD08    VP0    VE0     VP0    VE0
07     DA7     RD07    HP8    HE8     HP8    HE8
06     DA6     RD06    HP7    HE7     HP7    HE7
05     DA5     RD05    HP6    HE6     HP6    HE6
04     DA4     RD04    HP5    HE5     HP5    HE5
03     DA3     RD03    HP4    HE4     HP4    HE4
02     DA2     RD02    HP3    HE3     HP3    HE3
01     DA1     RD01    HP2    HE2     HP2    HE2
00     0       RD00    1      0       1      1
```

### 4.6 The COPCON danger bit

`COPCON` ($02E) bit 1 is `CDANG`. When CDANG=0, any Copper MOVE to a register in
$040..$07E is ignored — this is the "danger list" of the blitter control registers.
Setting CDANG=1 opens up the Copper's ability to run the blitter. Cleared on reset
(HRM p.1565).

### 4.7 A complete Copper list example

Verbatim from HRM p.1708:

```
COPPERLIST:
    ; set up pointers to two bitplanes
    DC.W BPL1PTH, $0002    ; move $0002 into register $0E0 (BPL1PTH)
    DC.W BPL1PTL, $1000    ; move $1000 into register $0E2 (BPL1PTL)
    DC.W BPL2PTH, $0002    ; move $0002 into register $0E4 (BPL2PTH)
    DC.W BPL2PTL, $5000    ; move $5000 into register $0E6 (BPL2PTL)

    ; colour registers
    DC.W COLOR00, $0FFF    ; white
    DC.W COLOR01, $0F00    ; red
    DC.W COLOR02, $00F0    ; green
    DC.W COLOR03, $000F    ; blue

    ; specify 2 lores bitplanes
    DC.W BPLCON0, $2200    ; 2 lores planes, colour on

    ; wait for line 150
    DC.W $9601, $FF00      ; VP=$96, HP=any

    ; change colour registers mid-display
    DC.W COLOR00, $0000    ; black
    DC.W COLOR01, $0FF0    ; yellow
    DC.W COLOR02, $00FF    ; cyan
    DC.W COLOR03, $0F0F    ; magenta

    ; end Copper list by waiting for the impossible
    DC.W $FFFF, $FFFE
```

This is a "raster split": the entire screen uses one set of four colours above line
150 and a different set below it. No CPU involvement.

### 4.8 Copper idioms

- **Raster split**: WAIT for the line; MOVE new colours (or BPLCONx).
- **Sprite reuse**: WAIT past last line of sprite's first use; MOVE new SPRxPTH/PTL
  to point at the second sprite image.
- **Mid-frame mode switch**: WAIT; MOVE BPLCON0 to swap HIRES/LORES, or DDFSTRT/STOP
  to narrow the fetch window, or DIWSTRT/STOP to shrink the visible area.
- **Copper loop**: Use COPJMP1 inside a Copper list to force the PC back to COP1LC
  (the assembler convention is to use CMOVE to write COPJMP1). This builds a tight
  polling loop (HRM p.1868 has a full example that raises an interrupt every 16
  lines).
- **Copper interrupt**: MOVE to INTREQ with bit 15 (SET/CLR) and bit 4 (COPER) set,
  yielding $8010, to request a level-3 interrupt (HRM p.1919).
- **Interlace field handling**: Maintain two Copper lists, switch COP1LC between
  them on VBlank depending on LOF. The graphics library calls them LOFCprList
  (long-frame) and SHFCprList (short-frame).

### 4.9 Copper-driven blit interlocking

If the Copper is used to start a blit, it *must* wait for the previous blit to
finish before starting another, because writing any BLTCONx register while the
blitter is running is undefined (HRM p.1994). This is what the WAIT's BFD bit is
for:

> "When the BFD bit is a 0, the logic of the Copper WAIT instruction is modified.
> The Copper will WAIT until the beam counter comparison is true and the blitter
> has finished." (HRM p.2002)

So the pattern is: `WAIT (with BFD=0)`; `MOVE BLTCON0..`; `MOVE BLTSIZE`. The
compare values for the WAIT can be anything past — e.g. `$0000, $0000` (WAIT for
line 0, H 0, with BFD=0) — which in practice is "wait for blitter only".

### 4.10 Starting and stopping the Copper

After reset, CPU code must:

1. Write a Copper list to Chip RAM.
2. Write its address (as a long) to `COP1LCH(a0)`.
3. Write anything to `COPJMP1(a0)` to force the PC to load.
4. Set `DMAF_SETCLR | DMAF_COPPER | DMAF_RASTER | DMAF_MASTER` in `DMACON` ($096)
   to enable Copper and bitplane DMA along with the master DMA bit.

(HRM p.1762 verbatim.)

The Copper is *stopped* by clearing COPEN in DMACON, or more commonly by simply
waiting on an impossible condition as the last instruction.

---

## 5. The Blitter

### 5.1 Overview

The Blitter lives inside Agnus (not Denise). It has *four* DMA channels:

- **A, B, C** — source channels.
- **D** — destination channel.

Each channel has its own pointer register `BLTxPTH/BLTxPTL` ($050..$057), its own
modulo `BLTxMOD` ($064..$066), and its own immediate-data register `BLTxDAT`
($074, $072, $070; note: BLTCDAT is written but not used for address, etc.). A
and B additionally have shift values (bits 15..12 of BLTCON0 and BLTCON1
respectively), A has first-word and last-word masks (`BLTAFWM` $044, `BLTALWM`
$046), and D has no mask or shift (HRM p.7724 onwards).

All four channels share *one* size register, `BLTSIZE` ($058). Writing BLTSIZE
**starts the blit** — this is crucial and must always be the last write during
blit setup (HRM p.7706).

The blitter only accesses Chip RAM. Attempting blits to Fast RAM will destroy Chip
RAM (HRM p.7700). The minimum cycle time is 4 ticks per word, maximum 8 ticks, both
at the system clock (7.16 MHz NTSC / 7.09 MHz PAL).

### 5.2 The function generator (LF code)

Every pixel of the destination is produced by an 8-bit look-up ("minterm") function
of the corresponding A, B, C bits:

```
A  B  C  →  truth table position
0  0  0  →  bit 0 (ABC')  ← actually ABC̄
0  0  1  →  bit 1 (ABC)
0  1  0  →  bit 2 (ABC)
0  1  1  →  bit 3 (ABC)
1  0  0  →  bit 4 (ABC̄)
1  0  1  →  bit 5 (AB̄C)
1  1  0  →  bit 6 (ABC̄)
1  1  1  →  bit 7 (ABC)
```

The 8-bit `LF` value sits in `BLTCON0[7..0]` and each bit says "do we want D=1
for this combination?" (HRM p.7917). Common values:

| Function     | LF  | Function    | LF  |
|--------------|-----|-------------|-----|
| D = A        | $F0 | D = AB      | $C0 |
| D = ¬A       | $0F | D = ¬AB     | $30 |
| D = B        | $CC | D = A∧¬B    | $0C |
| D = ¬B       | $33 | D = ¬A∧¬B   | $03 |
| D = C        | $AA | D = BC      | $88 |
| D = ¬C       | $55 | D = ¬BC     | $44 |
| D = A⊕C      | $A0 | D = ¬BC     | $22 |
| D = A⊕¬C     | $50 | D = ¬B∧¬C   | $11 |
| D = A+B      | $FC | D = A+B̄    | $F3 |
| D = A+C      | $FA | D = A+C̄    | $F5 |
| D = B+C      | $EE | D = B+C̄    | $DD |
| D = AB + ĀC  | $CA | (cookie-cut)|     |

(HRM Table 6-1 p.8046.)

**`$CA` — cookie cut.** A=mask, B=source, C=destination, D=destination. Wherever
the mask A is 1, copy B; wherever A is 0, leave C untouched. This is the classic
sprite-style blit used to render anything with transparency.

### 5.3 BLTCON0 and BLTCON1 — the control registers

**Area (normal) mode** (HRM p.11786):

```
BLTCON0                  BLTCON1
Bit Name                 Bit Name
15  ASH3                 15  BSH3
14  ASH2                 14  BSH2
13  ASH1                 13  BSH1
12  ASH0                 12  BSH0
11  USEA                 11  —
10  USEB                 10  —
 9  USEC                  9  —
 8  USED                  8  —
 7  LF7                   7  DOFF (disable D output)
 6  LF6                   6  —
 5  LF5                   5  —
 4  LF4                   4  EFE (exclusive fill enable)
 3  LF3                   3  IFE (inclusive fill enable)
 2  LF2                   2  FCI (fill carry input)
 1  LF1                   1  DESC (descending mode)
 0  LF0                   0  LINE = 0
```

Where:
- `ASH3..0` = A channel shift value 0..15 (right shift in ascending mode).
- `BSH3..0` = B channel shift value 0..15.
- `USEA..USED` = channel enable. Enabled channels do real DMA; disabled channels use
  the preloaded BLTxDAT register as the source word (HRM p.7746).
- `LF7..LF0` = the 8-bit minterm function value.
- `EFE` = exclusive fill enable.
- `IFE` = inclusive fill enable (see §5.6).
- `FCI` = fill carry input (starting state for the fill state machine, HRM p.8480).
- `DESC` = descending mode: pointers decrement, modulos subtract, shifts go left
  (HRM p.8297).
- `DOFF` = disable D output — blitter still computes, still sets the zero flag, but
  doesn't write (ECS-only in strict sense, HRM p.13829).

**Line mode** (`BLTCON1[0] = 1`) reinterprets most of the bits (HRM p.11824):

```
BLTCON0 (LINE)            BLTCON1 (LINE)
15 START3 (= x1 mod 16)   15 TEXTURE3
14 START2                 14 TEXTURE2
13 START1                 13 TEXTURE1
12 START0                 12 TEXTURE0
11 USEA=1 (fixed)         11 0
10 USEB=0 (fixed)         10 0
 9 USEC=1 (fixed)          9 0
 8 USED=1 (fixed)          8 0
 7 LF7                     7 0
 6 LF6                     6 SIGN
 5 LF5                     5 0 (reserved)
 4 LF4                     4 SUD ┐
 3 LF3                     3 SUL ├ octant
 2 LF2                     2 AUL ┘
 1 LF1                     1 SING (single-pixel per row)
 0 LF0                     0 LINE=1
```

Octant encoding (HRM p.11872):

```
OCTANT  SUD SUL AUL      BLTCON1 bits 4,3,2
  0      1   1   0           110
  1      0   0   1           001
  2      0   1   1           011
  3      1   1   1           111
  4      1   0   1           101
  5      0   1   0           010
  6      0   0   0           000
  7      1   0   0           100
```

### 5.4 Shifts and masks

The A and B channels each have a 16-entry barrel shifter that shifts data right
(ascending) or left (descending). The shift is free — it costs no extra cycles
(HRM p.8155). Zeros are shifted in *only* for the first word of the first row;
after that, the bits shifted out of the previous word are shifted in (HRM p.8160).

Because of this "carry between words" behaviour, the blitter also provides:

- `BLTAFWM` — first-word mask, ANDed with the first word of each row as fetched
  through the A channel (before shifting).
- `BLTALWM` — last-word mask, ANDed with the last word of each row.

When not using masks, initialise both to $FFFF. For line mode, both must be $FFFF.
If the blit is 1 word wide, both masks are applied simultaneously (HRM p.8200).

**Critical order-of-operations (HRM p.8281):** load shifts *before* data, because
loading BLTADAT/BLTBDAT immediately shifts the data. If you then change BSHIFT,
you don't get a re-shift of the old data — you get the old data shifted the wrong
way and the *next* data shifted the new way. Always set BLTCON0/BLTCON1 first.

### 5.5 Descending mode and overlap

Copies from a higher address to a lower address in the same bitmap work fine in
ascending mode; copies the other way (shifting the image down) will overwrite data
before you've read it. Descending mode:

- Decrement pointers by 2 per word fetch.
- Subtract modulos at end of each row.
- Shift left instead of right.
- First-word-mask applies to the last word in a row (which is now the first one
  fetched); last-word-mask applies to the first word in a row.

So to use descending mode you initialise each channel's pointer to the address of
the *last* word in the block. **Beware pre-decrement vs post-decrement:** the
Blitter uses the last-word address, not one past it, unlike 68000 pre-decrement
(HRM p.8309).

Rules of thumb (HRM p.8334):
1. A-disabled + cookie-cut function `$CA` for arbitrary rectangle copy with mask.
2. Ascending when shifting right; descending when shifting left.
3. Ascending when destination address is lower than source; descending otherwise.

### 5.6 Area fill mode

Either `IFE` (inclusive) or `EFE` (exclusive) in BLTCON1 turns on the fill engine.
The fill engine operates on the blitter's output after the minterm function and
fill only works correctly in **descending mode** (HRM p.8384).

The fill works like a 1-D state machine that walks right-to-left across each row.
`FCI` (fill-carry-input) is the initial state. Every time it sees a '1' bit in the
function output, it toggles its state. In its "1" state, it outputs 1s; in its "0"
state, it outputs 0s.

- **Inclusive fill (IFE)**: the boundary pixels stay set.
  ```
  before: 00100100-00011000
  after:  00111100-00011000
  ```
- **Exclusive fill (EFE)**: boundaries are consumed; the fill region is one pixel
  narrower.
  ```
  before: 00100100-00011000
  after:  00011100-00001000
  ```
- **`FCI = 1`** inverts the filled-vs-unfilled areas. The area "outside" the
  outlines ends up filled.

To get sharp single-point vertices, use EFE (HRM p.8458).

Area fill requires the input to have **only one set bit per horizontal row per
line boundary** — use line mode with `SING=1` (single-dot) to generate such outlines
before filling them.

### 5.7 Line mode in detail

Line mode turns the blitter into a Bresenham line rasteriser. The control registers
have the reinterpretations shown above. Setup (HRM p.8753 verbatim):

```
BLTADAT = $8000                        ; the traversing bit
BLTBDAT = line texture pattern ($FFFF for solid)
BLTAFWM = $FFFF
BLTALWM = $FFFF
BLTAMOD = 4 * (dy - dx)                ; Bresenham helper 1
BLTBMOD = 4 * dy                       ; Bresenham helper 2
BLTCMOD = width of bitplane in bytes   ; destination stride
BLTDMOD = width of bitplane in bytes   ; same
BLTAPT  = (4 * dy) - (2 * dx)          ; Bresenham accumulator
BLTCPT  = word containing first pixel
BLTDPT  = word containing first pixel
BLTCON0[15..12] = x1 mod 15            ; start bit for A
BLTCON0 SRCA = SRCC = SRCD = 1
BLTCON0 SRCB = 0
BLTCON0 LF = $CA  (AB + A'C, normal) or $4A (ABC̄ + A'C, XOR)
BLTCON1 LINEMODE = 1
BLTCON1 OVFLAG   = 0
BLTCON1[4..2]    = octant (from the table)
BLTCON1[15..12]  = start bit of texture pattern
BLTCON1 SIGNFLAG = (BLTAPT < 0)
BLTCON1 ONEDOT   = 1 if area-fill prep, else 0
BLTSIZE[15..6]   = dx + 1   (line length in pixels)
BLTSIZE[5..0]    = 2        (2 words wide, always, for line mode)
```

where `dx = max(|x2-x1|, |y2-y1|)`, `dy = min(|x2-x1|, |y2-y1|)`.

Conceptually, the A channel holds a "walking bit" ($8000) that the hardware shifts
across the screen, the B channel supplies the texture pattern, the C channel is the
current destination word, and D writes back. BLTAPT doubles as the Bresenham
accumulator. Each line step takes 8 clocks. BLTSIZE starts the blit.

The SING bit makes the blitter only set one pixel per horizontal row — use this to
produce closed outlines suitable for subsequent area fill (HRM p.8737).

### 5.8 Speed and DMA cycles

HRM Table 6-2 (p.8623) and the surrounding text describe how many cycles each
combination of enabled channels takes:

| USE bits (A B C D) | Active channels | Clocks per blit cycle |
|--------------------|-----------------|------------------------|
| $F (1111)          | A B C D         | 8 (A, B, C, D serialized) |
| $E (1110)          | A B C -         | 6 |
| $D (1101)          | A B - D         | 6 (paired) |
| $C (1100)          | A B - -         | 4 |
| $B (1011)          | A - C D         | 6 |
| $A (1010)          | A - C -         | 4 |
| $9 (1001)          | A - - D         | 4 |
| $8 (1000)          | A - - -         | 4 |
| $7 (0111)          | - B C D         | 6 |
| $6 (0110)          | - B C -         | 6 |
| $5 (0101)          | - B - D         | 6 |
| $4 (0100)          | - B - -         | 4 |
| $3 (0011)          | - - C D         | 4 |
| $2 (0010)          | - - C -         | 4 |
| $1 (0001)          | - - - D         | 4 |
| $0 (0000)          | none            | — |

(summary: use of A costs nothing over baseline; use of B always adds 2 clocks;
use of *both* C and D adds another 2 clocks — HRM p.8810). Line mode is 8 clocks
per pixel.

Rule-of-thumb timing:
```
copy with A,D:        4 * H * W / 7.16   microseconds   (NTSC)
copy with B,C,D:      8 * H * W / 7.16   microseconds   (NTSC)
```

### 5.9 Blitter nasty and priority

The blitter is higher priority than the 68000 on the Chip bus. Normally, when the
68000 has been unsatisfied for 3 consecutive memory cycles, Agnus releases one
cycle to it. Setting `DMAF_BLITHOG` (bit 10 of DMACON) turns on "blitter nasty"
mode, which makes the blitter keep the bus for every available Chip cycle
regardless of CPU starvation (HRM p.9067).

Display, audio, disk, refresh, and sprite DMA always have higher priority than the
blitter (HRM p.8854).

### 5.10 BLTSIZE, BLTSIZV, BLTSIZH

On OCS, `BLTSIZE` is 16 bits: 10-bit height and 6-bit width. Max blit is 1024×64
words = 1024×1024 pixels (HRM p.7791). Width of 0 means 64 words. Height of 0 means
1024 rows.

ECS adds `BLTSIZV` ($05C) and `BLTSIZH` ($05E) to extend the range to 32K×32K.

### 5.11 Blitter done flag and interrupts

`DMACONR` bit 14 = `BBUSY` (blitter busy); bit 13 = `BZERO` (last blit zero-flag
— stays 1 if all output bits were zero, useful for collision detection —
HRM p.8551).

Due to a pipelining quirk on pre-Fat-Agnus machines, BBUSY may not be set yet if
you test it immediately after writing BLTSIZE. The canonical wait-for-done sequence
(HRM p.8501):

```
btst.b  #DMAB_BLTDONE-8, DMACONR(al)
btst.b  #DMAB_BLTDONE-8, DMACONR(al)
```

The blitter also raises the `BLIT` interrupt (level 3, INTREQ bit 6) on completion.

The graphics.library provides `WaitBlit()` which does this correctly, including
hardware version detection.

### 5.12 Zero flag for collision detection

A blit with `D=AB` and D disabled (USED=0) sets the zero flag unless any pair of
bits both set — i.e. you can AND two bitmaps and if the result is all-zero, they
don't overlap. Read the result from DMACONR's BZERO bit after blit done (HRM
p.8550).

---

## 6. Color and palette

32 color registers, `COLOR00`..`COLOR31` at `$180..$1BE`. Each is 16 bits wide but
only the low 12 are used on OCS: 4 bits red, 4 bits green, 4 bits blue (HRM p.4491):

```
Bit  15 14 13 12  11 10  9  8   7  6  5  4   3  2  1  0
     X  X  X  X   R3 R2 R1 R0  G3 G2 G1 G0  B3 B2 B1 B0
```

All color registers are **write-only on OCS** (HRM p.4492). ECS and AGA add
readable and higher-bit-depth palettes, but those are outside the corpus.

- `COLOR00` is always the background colour. Anywhere no object is drawn, you see
  COLOR00. In genlock mode, COLOR00 is replaced by the external video signal
  (HRM p.4348).
- Single-playfield mode uses COLOR00..COLOR31 depending on depth (1..5 bits).
- Dual-playfield uses COLOR00..07 for PF1 and COLOR08..15 for PF2. In each, value
  0 is transparent.
- HAM uses COLOR00..15 for its "normal" output and modifies from the previous
  pixel for the hold-and-modify outputs.
- EHB uses COLOR00..31 for the first 32 colours and "halfbrite versions" of those
  for 32..63.
- Sprites use COLOR16..COLOR31 in groups of 4 (see §3.1).

The graphics.library function `LoadRGB4(vp, colortable, count)` writes ColorMap
entries; SetRGB4() writes individual ones. These are the API-level primitives;
underneath they end up as MOVEs to COLOR registers, often inside a Copper list.

---

## 7. graphics.library

### 7.1 Big picture

graphics.library is the middle layer between the raw custom chips and
intuition.library. Its responsibilities:

1. Maintain `GfxBase`, the library base (opened by `OpenLibrary("graphics.library")`).
2. Manage the display data structures:
   - `View` — the whole display (one per currently-displayed screen configuration).
   - `ViewPort` — a vertical slice of a View, with its own mode and colours.
   - `RasInfo` — what BitMap the ViewPort shows, and where the display is positioned.
   - `BitMap` — the raster memory layout (planes, dimensions, bytes-per-row).
   - `ColorMap` — the palette table.
3. Build Copper lists from the View → ViewPort → RasInfo chain: `MakeVPort(View*,
   ViewPort*)` generates a preliminary Copper list inside the ViewPort, `MrgCop(View*)`
   merges all ViewPorts' preliminary lists into the final `LOFCprList` and
   `SHFCprList` in the View, and `LoadView(View*)` pokes `COP1LC` to start it.
4. Provide the RastPort drawing model: a `RastPort` is a drawing context that
   references a BitMap and holds pens, modes, patterns, position, and font.
5. Provide the blitter-wrapped primitive operations: BltBitMap, BltPattern,
   BltTemplate, BltClear, ClipBlit, ScrollRaster, etc.
6. Provide higher-level primitives: SetAPen, Move, Draw, RectFill, Text,
   AreaMove/Draw/End, Flood.
7. Provide the VSprite / Bob animation system (chapter 3 of RKM L&D, not the focus
   here).
8. Provide `WaitBlit()`, `WaitTOF()` (wait for top of frame), and `WaitBOVP()`
   (wait for bottom of viewport) synchronisation primitives.
9. Own the Copper list management for Intuition screens — Intuition calls
   graphics.library under the hood for all raster operations.

### 7.2 The display data chain

```
       View  ─┬─> LOFCprList ──> (Copper list for long frame)
              └─> SHFCprList ──> (Copper list for short frame, interlace only)
        │
        └ViewPort─┬─>Next (list of ViewPorts vertically stacked)
                  ├─>RasInfo──>BitMap──>Planes[0..depth-1] (pointers to bitplanes)
                  │             BytesPerRow, Rows, Depth, Flags
                  │
                  ├─>ColorMap──>ColorTable[] (up to 32 UWORD entries)
                  ├─>DxOffset, DyOffset (where this VP sits in the View)
                  ├─>DWidth, DHeight
                  └─>Modes (HIRES, LACE, HAM, DUALPF, PFBA, SPRITES, ...)
```

The RasInfo has `RxOffset`, `RyOffset` — they select which part of a larger BitMap
is shown (scrolling). In dual-playfield mode RasInfo has a `Next` pointer to a
second RasInfo/BitMap (playfield 2).

### 7.3 Building a basic display

From RKM L&D p.1587 (cleaned):

```c
struct View v;
struct ViewPort vp;
struct BitMap b;
struct RasInfo ri;

InitView(&v);
InitVPort(&vp);
v.ViewPort = &vp;

InitBitMap(&b, DEPTH, WIDTH, HEIGHT);
for (int i = 0; i < DEPTH; i++) {
    b.Planes[i] = (PLANEPTR)AllocRaster(WIDTH, HEIGHT);
}

ri.BitMap   = &b;
ri.RxOffset = 0;
ri.RyOffset = 0;
ri.Next     = NULL;

vp.DWidth   = WIDTH;
vp.DHeight  = HEIGHT;
vp.RasInfo  = &ri;
vp.ColorMap = GetColorMap(4);
LoadRGB4(&vp, colortable, 4);

MakeVPort(&v, &vp);
MrgCop(&v);
LoadView(&v);
```

What each call does at the hardware level:

- `InitView` zeroes a View structure.
- `InitVPort` zeroes a ViewPort and sets defaults (Modes=0, Next=NULL).
- `InitBitMap` fills in BytesPerRow = ((Width+15)/16)*2, Rows=Height, Depth=Depth.
  It does *not* allocate the plane memory — the caller does that with
  `AllocRaster()` (which internally calls `AllocMem(MEMF_CHIP)` with padding).
- `GetColorMap(n)` allocates a ColorMap structure with a ColorTable of `n`
  entries, all initialised from a system default.
- `MakeVPort(&v, &vp)` builds a preliminary Copper list inside the ViewPort
  describing the colour changes, BPLCONx, DDFSTRT/STOP, DIWSTRT/STOP, and bitplane
  pointers this ViewPort needs.
- `MrgCop(&v)` walks the View's list of ViewPorts, merges their preliminary lists
  in beam-order into the final `LOFCprList` (and, if LACE, `SHFCprList`) at the
  top of the View.
- `LoadView(&v)` writes COP1LC and strobes COPJMP1, causing Agnus to use this
  Copper list from the next VBlank onwards.

On program exit you must call `FreeVPortCopLists(&vp)`, `FreeCprList(v.LOFCprList)`,
and `FreeCprList(v.SHFCprList)` (if LACE), plus free raster memory and ColorMap,
otherwise the system will crash when the Copper walks freed memory.

### 7.4 ViewPort.Modes flag bits

RKM L&D p.1267:

```
HIRES        — 640 horizontal pixels instead of 320
LACE         — interlaced (400 NTSC, 512 PAL vertical)
DUALPF       — two independent playfields
PFBA         — in dual playfield, PF2 in front of PF1 (i.e. PF2PRI=1)
HAM          — hold-and-modify mode
SPRITES      — sprites are in use in this ViewPort (tells system to reserve
               sprite colour registers)
VP_HIDE      — this ViewPort is obscured; don't generate display instructions
EXTRA_HALFBRITE — reserved (EHB) mode flag
```

### 7.5 The RastPort drawing model

A RastPort is a drawing context. It holds:

- `BitMap` — pointer to the bitmap being drawn into.
- Drawing pens: `FgPen` (aka A-Pen, primary), `BgPen` (B-Pen), `AOlPen` (area
  outline).
- `DrawMode` — JAM1, JAM2, COMPLEMENT, INVERSEVID.
- `LinePtrn` — 16-bit pattern for patterned line drawing.
- `AreaPtrn` — pointer to a multi-word pattern for area fills, plus `AreaPtSz`
  (a power of two giving the height).
- `Font` — `TextFont *` for text rendering.
- `AreaInfo`, `TmpRas` — scratch structures for area fill.
- `GelsInfo` — for VSprite/Bob animation.
- `Mask` — write mask; selects which bitplanes drawing actually affects.
- `cp_x`, `cp_y` — current pen position for Move/Draw.

Initialisation:

```c
struct RastPort rp;
InitRastPort(&rp);
rp.BitMap = &mybitmap;
```

Primitive drawing calls all take a RastPort pointer (Autodocs):

- `SetAPen(rp, pen)` / `SetBPen(rp, pen)` / `SetOPen(rp, pen)` — set pens.
- `SetDrMd(rp, mode)` — JAM1 | JAM2 | COMPLEMENT | INVERSEVID.
- `WritePixel(rp, x, y)` — plot a single pixel using FgPen.
- `ReadPixel(rp, x, y)` — returns pen value or -1 if out of range.
- `Move(rp, x, y)` — set pen position (no drawing).
- `Draw(rp, x, y)` — draw line from current pen to (x,y), advance pen.
- `PolyDraw(rp, count, array)` — draw a polyline from an array of (x,y) pairs.
- `RectFill(rp, xmin, ymin, xmax, ymax)` — fill a rectangle.
- `SetRast(rp, pen)` — set entire raster to a pen value (uses blitter).
- `ScrollRaster(rp, dx, dy, xmin, ymin, xmax, ymax)` — hardware-accelerated scroll
  of a region, wrapping around.
- `Text(rp, string, length)` — render text with the current font.
- `AreaMove(rp, x, y)` — start a new polygon for area fill.
- `AreaDraw(rp, x, y)` — add a vertex.
- `AreaEnd(rp)` — actually rasterise the accumulated polygon using the blitter.
- `Flood(rp, mode, x, y)` — flood-fill from a seed point until hitting OPen colour
  (mode 0) or a pixel whose colour differs (mode 1).

Cautions from the corpus:

> "If you attempt to draw a line outside the bounds of the BitMap, using the basic
> initialized RastPort, you may crash the system. You must either do your own
> software clipping to assure that the line is in range, or use the layer library."
> (RKM L&D p.2644)

A RastPort can be a bare drawing surface (you clip yourself) or you can attach it to
a Layer, in which case the layers library installs its `ClipRect` chain and all
drawing is automatically clipped to the visible portions of that layer.

### 7.6 Blitter-wrapped operations

These sit on top of the raw blitter and implement the usual rectangle-rectangle
operations in a bitplane-aware way (Autodocs):

- `BltBitMap(srcBitMap, srcX, srcY, dstBitMap, dstX, dstY, sizeX, sizeY, minterm,
   mask, tempbuffer)` — copy a rectangular region from one BitMap to another,
  applying the 8-bit minterm function. `mask` selects which planes. `tempbuffer`
  is only used if src and dst overlap in a way that prevents direct blitting.
- `BltBitMapRastPort(srcBitMap, srcX, srcY, dstRP, dstX, dstY, w, h, minterm)` —
  same, but destination is a RastPort so it respects clipping (layers). If the
  destination layer has ClipRects, this dispatches through the clipping code.
- `BltClear(ptr, bytecount, flags)` — zero a block of memory using the blitter.
  Flags=0 uses ascending mode; flags=1 is synchronous.
- `BltPattern(rp, mask, xmin, ymin, xmax, ymax, bytecount)` — fill a region using
  the current area pattern.
- `BltTemplate(source, srcX, srcMod, destRP, destX, destY, sizeX, sizeY)` — "cookie
  cut" a 1-bit-deep template into the destination using the current pens and draw
  mode. Used for text rendering and for masked shapes.
- `ClipBlit(srcRP, srcX, srcY, destRP, destX, destY, sizeX, sizeY, minterm)` —
  copy between RastPorts, clipping against both sets of layers.
- `Flood(rp, mode, x, y)` — blitter-assisted flood fill, but actually runs in CPU
  scan code for the boundary walk.
- `ScrollRaster(rp, dx, dy, xmin, ymin, xmax, ymax)` — shift a region in a RastPort.

### 7.7 Double-buffering

The canonical pattern uses two BitMaps and swaps the BPLxPT MOVE instructions in the
Copper list:

```c
struct BitMap *bitmaps[2];
struct RasInfo ri;
int cur = 0;

/* ... set up everything with ri.BitMap = bitmaps[0], MakeVPort, MrgCop, LoadView ... */

/* per frame: */
WaitTOF();                           /* sync to top of frame */
DrawFrame(&rp[1 - cur]);             /* render into the hidden buffer */
ri.BitMap = bitmaps[1 - cur];        /* swap */
MakeVPort(&v, &vp);                  /* rebuild preliminary Copper list */
MrgCop(&v);                          /* merge into LOFCprList/SHFCprList */
LoadView(&v);                        /* reload pointer at VBlank */
cur = 1 - cur;
```

In practice, performance-sensitive code pre-computes two Copper lists and just
switches COP1LC directly. For Intuition-based apps, the Workbench's screen
management does it for you if you ask `OpenScreenTags` for double buffering.

### 7.8 GfxBase notable fields

From corpus (Autodocs / gfxbase.h):

- `ActiView` — pointer to the currently-displayed View. Save this and restore on exit
  if you're stealing the display (RKM L&D p.1817).
- `default_monitor` — pointer to a graphics monitor structure describing the
  current monitor (ECS/AGA).
- `copinit` — pointer to the system's built-in power-on Copper list.
- `DefaultFont` — topaz.font, used by newly-opened RastPorts by default.
- `DisplayFlags` — PAL/NTSC/GENLOC flags.
- `HashTable` — sprite allocation hash.
- `LOFlist`, `SHFlist` — the system's own long-frame and short-frame Copper
  list heads, merged into the live display.

---

## 8. intuition.library

### 8.1 Big picture

Intuition is the user interface layer. It layers screens, windows, gadgets, menus,
and requesters on top of graphics.library and layers.library, and routes user input
via an Intuition Direct Communications Message Port (IDCMP) on each window.

Key data structures:

- **Screen** (`<intuition/screens.h>`): the top-level display unit. Contains an
  embedded `ViewPort`, `RastPort`, `BitMap`, and `LayerInfo`. Screens determine the
  display resolution, depth, and palette. All windows *inside* a screen share that
  screen's video mode.
- **Window**: a rectangle (layer) inside a screen, with its own RastPort, optional
  borders, optional system gadgets (close, drag, depth, sizing), and an optional
  IDCMP port for input.
- **Gadget**: a clickable control — button, checkbox, proportional slider, text
  entry, etc. Gadgets are attached to either Windows or Requesters.
- **Menu / MenuItem / SubItem**: a vertical bar menu strip, attached to a window
  via `SetMenuStrip()`. When the user presses the right mouse button, Intuition
  suspends drawing in all windows and displays the active window's menus.
- **Requester**: a modal dialog attached to a window or a screen (for "System
  Requesters" like "please insert volume X").
- **IDCMP**: a message port built into a Window struct where Intuition delivers
  IntuiMessage events. The application reads them with `GetMsg()` on
  `window->UserPort`.

### 8.2 Screens

A Screen is effectively a managed ViewPort with built-in graphics library
structures:

```c
struct Screen {
    struct Screen *NextScreen;
    struct Window *FirstWindow;
    WORD LeftEdge, TopEdge, Width, Height;
    WORD MouseY, MouseX;
    UWORD Flags;
    UBYTE *Title, *DefaultTitle;
    BYTE BarHeight, BarVBorder, BarHBorder, MenuVBorder, MenuHBorder;
    BYTE WBorTop, WBorLeft, WBorRight, WBorBottom;
    struct TextAttr *Font;
    struct ViewPort ViewPort;
    struct RastPort RastPort;
    struct BitMap BitMap;
    struct LayerInfo LayerInfo;
    struct Gadget *FirstGadget;
    UBYTE DetailPen, BlockPen;
    UWORD SaveColor0;
    struct Layer *BarLayer;
    UBYTE *ExtData, *UserData;
};
```

(RKM 3rd p.2022.)

Screens come in two kinds (RKM 3rd p.1951):

- **Public screens** — can be shared between applications. The Workbench screen is
  the oldest public screen. Any `CUSTOMSCREEN | PUBLICSCREEN` with a name becomes
  public.
- **Custom screens** — private to the creating application.

Create with `OpenScreen(&newScreen)` (v1.3) or `OpenScreenTagList(NULL, tags)` /
`OpenScreenTags(NULL, SA_Depth, 2, SA_Pens, (ULONG)pens, TAG_DONE)` (v2.0+).
Close with `CloseScreen(screen)` (returns BOOL in v2, which is FALSE if the
public screen still has users).

Screen tag items (RKM 3rd §3):

- `SA_Left`, `SA_Top`, `SA_Width`, `SA_Height` — position and size.
- `SA_Depth` — bitplane depth.
- `SA_DisplayID` — display mode key (NTSC/PAL/HIRES/LACE/HAM/etc.).
- `SA_Pens` — pen specification for "3D look".
- `SA_DetailPen`, `SA_BlockPen` — classic pen fields.
- `SA_Title`, `SA_FontData`, `SA_SysFont`.
- `SA_Colors` — ColorSpec array for initial palette.
- `SA_Type` — CUSTOMSCREEN | PUBLICSCREEN.
- `SA_PubName` — public screen name.
- `SA_ErrorCode` — where to store error return.

### 8.3 Screen-to-ViewPort relationship

The embedded `Screen.ViewPort` is a real graphics.library ViewPort. Intuition owns
it: you must not call `MakeVPort()` on it yourself, but you can read its
fields to ask questions like "where does my screen currently live?". Intuition
internally calls graphics to maintain the copper list for the screen. Your
ViewPort.Modes is determined by the tags you passed at open time.

You can access the Screen's RastPort to draw directly into the screen (behind all
windows), though the usual pattern is to draw into a Window's RastPort.

### 8.4 Windows

A Window is a rectangle inside a Screen. Intuition creates a Layer for each Window
(via layers.library), and the Layer in turn manages the clipping so that drawing
into the Window's RastPort only affects the visible portion(s).

Window flags (partial — see `<intuition/intuition.h>`):

- `WINDOWSIZING`, `WINDOWDRAG`, `WINDOWDEPTH`, `WINDOWCLOSE` — system gadgets.
- `SIZEBRIGHT`, `SIZEBBOTTOM` — where the sizing gadget sits.
- `SMART_REFRESH`, `SIMPLE_REFRESH`, `SUPER_BITMAP` — refresh mode (these map to
  the underlying layer type).
- `BACKDROP` — always behind all other windows.
- `BORDERLESS`.
- `GIMMEZEROZERO` — shift the window's RastPort origin so that (0,0) is inside the
  content area, not the border; costs memory because Intuition allocates a second
  inner layer.
- `ACTIVATE` — make this window the active one on open.
- `RMBTRAP` — do *not* let the right mouse button pop menus for this window
  (intercept instead).
- `NOCAREREFRESH` — Intuition won't send refresh events to this window's IDCMP.

Create with `OpenWindow(&newWindow)` (v1.3) or `OpenWindowTags(NULL, ...)` (v2+).
Close with `CloseWindow(window)`.

Refresh modes (these map directly to the Layer types — see §9):

- `SMART_REFRESH` — the system backs up obscured portions into a hidden bitmap.
- `SIMPLE_REFRESH` — obscured portions are discarded; when revealed, Intuition
  sends a `REFRESHWINDOW` IDCMP event and your code must redraw from scratch.
- `SUPER_BITMAP` — you supply a bitmap larger than the visible area; Intuition
  always draws into the back-up; scrolling the window scrolls across the larger
  bitmap.

### 8.5 The IDCMP

Each Window has an optional message port, `window->UserPort`, which Intuition uses
to send IntuiMessage events to the application. An `IntuiMessage` is an
`exec.library` Message with extra fields:

```c
struct IntuiMessage {
    struct Message ExecMessage;
    ULONG Class;          /* IDCMP event class */
    UWORD Code;           /* details (e.g. menu number, key code) */
    UWORD Qualifier;      /* keyboard qualifiers, mouse buttons */
    APTR  IAddress;       /* pointer to gadget, etc. */
    WORD  MouseX, MouseY;
    ULONG Seconds, Micros;
    struct Window *IDCMPWindow;
    struct IntuiMessage *SpecialLink;
};
```

Common `Class` values:

- `MOUSEBUTTONS` — left/right button state change (Code = SELECTDOWN/SELECTUP/
  MENUDOWN/MENUUP).
- `MOUSEMOVE` — pointer movement (only if REPORTMOUSE is set).
- `GADGETDOWN` / `GADGETUP` — gadget manipulation; IAddress = Gadget*.
- `CLOSEWINDOW` — close gadget clicked.
- `MENUPICK` — menu selection; Code has the MenuNumber (selected menu/item/subitem).
- `REFRESHWINDOW` — simple-refresh window needs redrawing.
- `NEWSIZE` — window has been resized.
- `ACTIVEWINDOW` / `INACTIVEWINDOW`.
- `RAWKEY` / `VANILLAKEY` — keyboard events.
- `DISKINSERTED` / `DISKREMOVED`.
- `REQSET` / `REQCLEAR` — a requester opened/closed.

The typical event loop:

```c
ULONG signals = Wait(1L << window->UserPort->mp_SigBit);
while ((msg = (struct IntuiMessage *)GetMsg(window->UserPort))) {
    ULONG class = msg->Class;
    UWORD code  = msg->Code;
    ReplyMsg((struct Message *)msg);   /* do this ASAP */
    switch (class) {
        case CLOSEWINDOW: goto done;
        case MENUPICK:    handle_menu(code); break;
        /* ... */
    }
}
```

**You must Reply to every IntuiMessage** before closing the window, otherwise the
system deadlocks; `ModifyIDCMP(window, 0)` or Intuition's close code explicitly
checks for unreplied messages.

### 8.6 Gadgets

Gadgets are controls with typed behaviour:

- `BOOLGADGET` — pressable button; sends GADGETUP on release.
- `STRGADGET` — text input field.
- `PROPGADGET` — proportional slider / knob.
- `CUSTOMGADGET` (v2+) — BOOPSI class-based custom gadget.

System gadgets (added automatically based on Window flags):

- Close gadget (upper-left).
- Drag gadget (title bar).
- Depth gadget (upper-right, front/back toggle).
- Zoom gadget (v2, next to depth).
- Sizing gadget (lower-right corner, or wherever SIZEB* flags put it).

Create with `AddGadget()` or directly embedded into the NewWindow's FirstGadget
chain. Modify with `ModifyProp()`, `RefreshGadgets()`.

### 8.7 Menus, Requesters

A menu strip is a linked list of `Menu` structs, each with a linked list of
`MenuItem` structs, each with an optional linked list of sub-`MenuItem` structs.
`SetMenuStrip(window, menu)` attaches; `ClearMenuStrip(window)` detaches.

Requesters come in two flavours: application-defined (`Request()`/`EndRequest()`
on a Requester struct), and system requesters (`AutoRequest()` for simple
"Yes/No/Retry/Cancel" dialogs, `BuildSysRequest()` / `FreeSysRequest()` for
custom). A window with an active requester ignores normal input until the
requester ends.

### 8.8 Input flow

1. CIA-A counter on joystick lines ticks as the mouse moves (via the quadrature
   inputs to JOY0DAT — HRM p.10268).
2. `gameport.device` reads those counters and the fire button states, producing
   `InputEvent` records.
3. `keyboard.device` produces similar InputEvent records for key presses.
4. `input.device` receives the merged event stream and sends it to Intuition (and
   any other handlers in its chain, in priority order).
5. Intuition's input handler interprets the events: it updates the mouse pointer,
   checks which window the pointer is over (via `WhichLayer()` from
   layers.library), handles gadget/menu hit-tests, and if nothing else claims the
   event it posts an IDCMP message to the active window's UserPort.
6. The application receives the message via `GetMsg()`.

Key point for emulation: the event plumbing goes through standard Exec message
passing, so the fast path for mouse movement is *not* "mouse moves, CPU
interrupts, Intuition updates pointer sprite". It is more like "CIA-A counters
tick, input.device runs at VBlank (and when woken by the CIA interrupt), Intuition
moves the pointer sprite in the copper list for the current screen".

### 8.9 System screens and Workbench

At boot, after the OS has loaded, Intuition opens the Workbench Screen (a public
screen with a default palette and resolution from Preferences). Applications can
either:

- **Open a window on the Workbench screen** by passing a `Screen *` of NULL or
  the Workbench screen pointer.
- **Open a custom screen** and then open a window on it.

---

## 9. layers.library

### 9.1 What layers do

layers.library implements overlapping rectangular regions inside a shared BitMap
(usually a screen's bitmap). Each Layer is an independently-manageable drawing
region — it has a RastPort, a list of ClipRects describing its visible areas, and
a type that determines what happens when another layer obscures it.

Layer types (RKM L&D p.4233):

- **Simple refresh (LAYERSIMPLE)** — no backup. Obscured portions are discarded;
  when uncovered, the layer is marked `LAYER_REFRESH` and the application must
  redraw them. Cheapest on memory, but the application must repaint.
- **Smart refresh (LAYERSMART)** — the system automatically maintains a backup
  bitmap for obscured portions. Drawing into the layer goes both to the visible
  screen bitmap and to the backup bitmap. When the layer is uncovered, the
  backup is blitted back. Expensive in memory but transparent to the app.
- **Superbitmap (LAYERSUPER)** — like smart refresh, but the backup is a *larger*
  bitmap the application provides. The layer shows a window into it, and
  `ScrollLayer()` moves that window.
- **Backdrop (LAYERBACKDROP)** — always behind all non-backdrop layers. Can be
  combined with any refresh type.

### 9.2 Layer_Info and ClipRects

Each drawing area (typically a screen's BitMap) needs a `Layer_Info` structure that
tracks the list of layers. `NewLayerInfo()` (v1.1+) allocates one; older code used
`InitLayers()`. Each Layer has a `ClipRect` list — a sequence of non-overlapping
rectangles describing the visible portions of the layer. When a layer is moved,
sized, or depth-arranged, layers.library regenerates the ClipRect lists for the
affected layers.

When graphics.library renders into a RastPort that belongs to a Layer, it walks
the ClipRect list and clips the blit/line/fill to each rect in turn — this is
what makes drawing "through" overlapping windows work without each app having to
think about it.

### 9.3 Layer operations

- `CreateUpfrontLayer(layerInfo, bitmap, x0, y0, x1, y1, flags, superBitMap)` —
  create a new layer in front of all others.
- `CreateBehindLayer(...)` — same, but behind.
- `DeleteLayer(dummy, layer)` — remove a layer.
- `MoveLayer(dummy, layer, dx, dy)` — move to new position.
- `SizeLayer(dummy, layer, dx, dy)` — resize.
- `ScrollLayer(dummy, layer, dx, dy)` — for superbitmap, change which part of the
  backing bitmap is visible.
- `UpfrontLayer(dummy, layer)` / `BehindLayer(dummy, layer)` — depth-arrange.
- `WhichLayer(layerInfo, x, y)` — return the topmost layer at a point. Intuition
  uses this for mouse hit-testing.
- `LockLayer(dummy, layer)` / `UnlockLayer(layer)` — prevent drawing during an
  operation. Locks must be nested inside LockLayerInfo/UnlockLayerInfo if you
  lock multiple layers, to avoid deadlock.
- `LockLayers(layerInfo)` / `UnlockLayers(layerInfo)` — lock all layers in a
  Layer_Info at once. Intuition uses this while drawing menus via
  SwapBitsRastPortClipRect.
- `SwapBitsRastPortClipRect(rp, clipRect)` — swap the on-screen bits with the
  off-screen ClipRect backing bits. Used for menu rendering: render the menu in
  a back-up area, then swap it on-screen, swap back when the menu closes.

### 9.4 Damage lists and BackFill

When a layer is uncovered or resized, the exposed area is added to the layer's
"damage list" (a set of ClipRects pending redraw). For smart-refresh layers, the
system then blits the backup into those rects. For simple-refresh layers, it
marks `LAYER_REFRESH` and (via Intuition) sends `REFRESHWINDOW` to the app.

A layer can have a `BackFill` hook — a function called on exposed areas to fill
them with a custom pattern instead of the default. Workbench uses this for
pattern backdrops.

---

## 10. Tying it together

### 10.1 What happens when you draw to a Window

Pseudocode-style trace for `Draw(window->RPort, 100, 50)`:

1. `Draw()` enters graphics.library, sees the RastPort belongs to a Layer (the
   Layer field in the RastPort is non-NULL).
2. graphics calls into layers.library's clipping path.
3. layers.library walks the Layer's ClipRect list. For each ClipRect:
   a. Translate window-local coordinates into screen-bitmap coordinates.
   b. Clip the line against the ClipRect.
   c. If the ClipRect is in the on-screen bitmap, call the blitter (or CPU
      line-draw code) to actually draw into the screen's bitmap.
   d. If the layer is smart-refresh or superbitmap and the ClipRect is in a
      backing bitmap, also draw into the backing bitmap.
4. For a blitter-using operation, graphics calls `OwnBlitter()` /
   `WaitBlit()` / `DisownBlitter()` to serialise blitter usage between tasks.
5. The blitter runs; when done it raises level-3 BLIT interrupt; graphics
   wakes up the waiting task.
6. On the next VBlank, the display shows the new pixel — but note: nothing
   about rendering itself is synchronised to VBlank, you just happen to see
   the new bits because the Copper-driven display fetches them from the same
   bitmap on the next line.

### 10.2 What happens when the user clicks a window

1. Mouse movement generates CIA-A quadrature ticks and JOYxDAT updates.
2. gameport.device sees the JOY counter change, generates an IECLASS_RAWMOUSE
   InputEvent with the delta and current button state.
3. input.device merges the event into its event queue and wakes the handler
   chain. Intuition's handler is at the top of the chain.
4. Intuition updates the pointer sprite's SPRxPOS register (usually by
   rewriting a pair of MOVEs in the system's Copper list inside the graphics
   VBlank server). Depending on version, this may happen via a direct Copper
   list edit or via graphics.library's MoveSprite().
5. On click, Intuition asks layers.library's WhichLayer() which layer is at
   the click position.
6. If the click is on a system gadget or a user gadget, Intuition handles it
   (drag, depth-change, close). Otherwise Intuition builds an IntuiMessage
   with Class=MOUSEBUTTONS and posts it to the layer's window's IDCMP
   UserPort.
7. The application, blocked on Wait() for its UserPort signal, wakes up,
   calls GetMsg(), handles the click, and ReplyMsg()'s.

### 10.3 The whole picture (one VBlank interval)

At VBlank:

1. Agnus asserts VERTB. Paula latches the interrupt in INTREQ bit 5.
2. The 68000 sees IPL=3, takes the interrupt, runs the VBlank server chain.
3. The graphics library's VBlank server runs first (highest priority):
   - For each dynamic ViewPort, rebuilds or reloads the appropriate Copper
     list (if ViewPort state changed).
   - Rewrites COP1LC and strobes COPJMP1 to start the Copper from the top.
   - Updates sprite positions (MoveSprite-pending requests).
   - Advances any animation (Bob/VSprite) system.
4. Intuition's VBlank server runs, doing pointer sprite updates, menu state,
   blinking cursors.
5. input.device's VBlank hook (if any) runs to time-stamp events.
6. The OS returns from interrupt. Meanwhile:
7. Agnus, at the end of VBlank, re-loads its Copper PC from COP1LC regardless
   (this happens in hardware, not software). The Copper starts running its
   list.
8. The Copper's first MOVEs reload BPLxPT, set up BPLCONx, DDFSTRT/DDFSTOP,
   DIWSTRT/DIWSTOP for the first ViewPort.
9. At line ~21 (NTSC), VBlank ends; the bitplane DMA starts fetching; Denise
   serialises; pixels start appearing.
10. If there are more ViewPorts in the View, the Copper WAITs for each
    ViewPort's vertical position and MOVEs new colour registers and BPLCONx
    there.
11. At the bottom of the visible area, the Copper's final WAIT for
    `$FFFF, $FFFE` (impossible) parks it until VBlank.

---

## 11. Display timing reference

### 11.1 NTSC vs PAL line counts

Extracted from HRM §Beam position and Playfield hardware:

| Quantity                     | NTSC         | PAL          |
|------------------------------|--------------|--------------|
| Master oscillator            | 28.37516 MHz | 28.37516 MHz (A500/A2000 PAL), A1000 used different crystal |
| System clock                 | 7.15909 MHz  | 7.09379 MHz  |
| Colour-clock rate            | 3.579545 MHz | 3.546895 MHz |
| Colour clock length          | 279.36 ns    | 281.94 ns    |
| Line length                  | 227.5 CC     | 227.5 CC     |
| Line length (alternating)    | 227/228 CC   | 227 CC constant |
| Lines per field              | 262/263      | 312/313      |
| Frame rate                   | ~59.94 Hz    | ~50 Hz       |
| Visible lines (non-interlace)| 200 (241 max)| 256 (283 max)|
| Visible lines (interlace)    | 400 (483 max)| 512 (567 max)|
| Default DIWSTRT              | $2C81        | $2C81        |
| Default DIWSTOP              | $F4C1        | $2CC1        |
| VBlank stop line             | $15 (21)     | $1D (29)     |

(HRM Table 3-13 p.3887.)

### 11.2 DMA slots per horizontal line

HRM p.8873:

```
 4 cycles  memory refresh (fixed)
 3 cycles  disk DMA
 4 cycles  audio DMA (1 word per channel × 4 channels)
16 cycles  sprite DMA (2 words per channel × 8 channels)
80 cycles  bitplane DMA (max, for 4 hires planes or equivalent)
```

Total: 107 cycles committed in the worst case, out of 226 (one line). Everything
else is available to the blitter, Copper and 68000.

In a 4-bitplane lores display, bitplane DMA takes 80 slots but only on every
second cycle, leaving half the slots for the 68000. In a 6-bitplane lores
display, bitplane DMA takes all 6 × 20 = 120 even slots per line, reducing 68000
throughput during display time. In a **4-bitplane hires display**, bitplane DMA
takes 4 × 40 = 160 slots per line, which effectively locks out the 68000, the
blitter, and the Copper during display (HRM p.9026).

### 11.3 The magic constants (why 320 and 640)

A normal scan line has 227.5 colour clocks. Subtracting horizontal blanking
(cycles ~$0F..$35, roughly 39 colour clocks) leaves ~188 colour clocks of
potential visible video. Minus 8 clocks of overscan border on each side leaves
roughly 160 colour clocks of "centre of screen", which is exactly 320 lores
pixels or 640 hires pixels.

- Default **DDFSTRT=$38** in lores, fetching 20 words from positions $38, $40,
  ..., $D0: `((0xD0-0x38)/8)+1 = 20` words = 20×16 lores pixels = 320 lores
  pixels.
- Default **DDFSTRT=$3C** in hires, fetching 40 words from positions $3C, $40,
  ..., $D4: `((0xD4-0x3C)/4)+1 = 40` words = 40×16 hires pixels = 640 hires
  pixels.

Hardware-maximum horizontal fetch is DDFSTRT=$18..DDFSTOP=$D8: 25 words in lores
(400 pixels), 49 words in hires (784 pixels). The usable display, though, is
limited by horizontal blanking to 368 lores pixels / 23 words (HRM p.3893).

### 11.4 Max bitplanes vs bandwidth

- Lores 1..6 planes: always fits — up to 120 DMA slots out of 226.
- Hires 1..4 planes: up to 160 DMA slots out of 226. 5–6 planes: **not possible**
  on OCS because there aren't enough slots.
- Dual playfield: 3+3 planes lores, 2+2 planes hires.

---

## Appendix A — Copper instruction encoding

Verbatim from HRM p.2039 (Table 2-2):

```
         MOVE            WAIT            SKIP
Bit    IR1     IR2     IR1     IR2    IR1     IR2
15     X       RD15    VP7     BFD    VP7     BFD
14     X       RD14    VP6     VE6    VP6     VE6
13     X       RD13    VP5     VE5    VP5     VE5
12     X       RD12    VP4     VE4    VP4     VE4
11     X       RD11    VP3     VE3    VP3     VE3
10     X       RD10    VP2     VE2    VP2     VE2
09     X       RD09    VP1     VE1    VP1     VE1
08     DA8     RD08    VP0     VE0    VP0     VE0
07     DA7     RD07    HP8     HE8    HP8     HE8
06     DA6     RD06    HP7     HE7    HP7     HE7
05     DA5     RD05    HP6     HE6    HP6     HE6
04     DA4     RD04    HP5     HE5    HP5     HE5
03     DA3     RD03    HP4     HE4    HP4     HE4
02     DA2     RD02    HP3     HE3    HP3     HE3
01     DA1     RD01    HP2     HE2    HP2     HE2
00     0       RD00    1       0      1       1

  X   = don't care (zero for compatibility)
  IR1 = first instruction word
  IR2 = second instruction word
  DA  = destination address / 2
  RD  = data to move
  VP  = vertical beam position
  HP  = horizontal beam position
  VE  = vertical enable mask bit
  HE  = horizontal enable mask bit
  BFD = blitter-finished disable (1 = ignore blitter status)
```

### Decoding rules
- Bit 0 of IR1 = 0 → MOVE.
- Bit 0 of IR1 = 1, Bit 0 of IR2 = 0 → WAIT.
- Bit 0 of IR1 = 1, Bit 0 of IR2 = 1 → SKIP.

### Timing rules
- MOVE: 2 DMA cycles (4 colour clocks in practice because of odd-cycle
  constraint).
- WAIT: 3 DMA cycles (6 colour clocks) — "wake up" cycle.
- SKIP: 2 DMA cycles (4 colour clocks).

### CMOVE / CWAIT macros (from graphics/copper.h)
- `CMOVE(uc, reg, val)` — add a MOVE to a user Copper list structure.
- `CWAIT(uc, vline, hpos)` — add a WAIT to a user Copper list.
- `CEND(uc)` — terminate a user Copper list.

### Protected registers
- Copper can write $080..$1FE unconditionally.
- Copper can write $040..$07E iff `CDANG` (COPCON bit 1) is set.
- Copper can never write $000..$03E.
- CDANG is cleared by reset.

---

## Appendix B — BLTCON0 / BLTCON1 bit tables

### Area (normal) mode (LINE = 0)

```
BLTCON0 ($040)                 BLTCON1 ($042)
Bit  Name    Meaning           Bit  Name    Meaning
15   ASH3 ┐                    15   BSH3 ┐
14   ASH2 │ A shift 0..15      14   BSH2 │ B shift 0..15
13   ASH1 │                    13   BSH1 │
12   ASH0 ┘                    12   BSH0 ┘
11   USEA   Enable A fetch     11   —
10   USEB   Enable B fetch     10   —
 9   USEC   Enable C fetch      9   —
 8   USED   Enable D write      8   —
 7   LF7 ┐                      7   DOFF  Disable D output (ECS)
 6   LF6 │                      6   —
 5   LF5 │                      5   —
 4   LF4 │ Minterm bits         4   EFE   Exclusive fill enable
 3   LF3 │                      3   IFE   Inclusive fill enable
 2   LF2 │                      2   FCI   Fill carry input
 1   LF1 │                      1   DESC  Descending mode
 0   LF0 ┘                      0   LINE  = 0 (area mode)
```

### Line mode (LINE = 1)

```
BLTCON0 ($040)                 BLTCON1 ($042)
Bit  Name                      Bit  Name
15   START3 ┐                  15   TEXTURE3 ┐
14   START2 │ x1 mod 16         14   TEXTURE2 │ texture start bit
13   START1 │                   13   TEXTURE1 │
12   START0 ┘                   12   TEXTURE0 ┘
11   1      (USEA forced)       11   0
10   0      (USEB forced)       10   0
 9   1      (USEC forced)        9   0
 8   1      (USED forced)        8   0
 7   LF7    set to support       7   0
 6   LF6    AB+A'C ($CA) or      6   SIGN   Bresenham sign
 5   LF5    ABC'+A'C ($4A)       5   0      reserved
 4   LF4    for XOR              4   SUD    octant bit 2
 3   LF3                         3   SUL    octant bit 1
 2   LF2                         2   AUL    octant bit 0
 1   LF1                         1   SING   single-dot per row
 0   LF0                         0   LINE = 1
```

### USE-bit-to-cycle-count table (HRM Table 6-2)

```
USE      Active             Cycles  Example trace
$F       A B C D            8       AO BO CO -A1 B1 C1 DO A2 B2 C2 D1 D2
$E       A B C              6       AO BO CO A1 B1 C1 A2 B2 C2
$D       A B     D           6       AO BO -A1 B1 DO A2 B2 D1 -D2
$C       A B                 4       AO BO -A1 B1 - A2 B2
$B       A     C D           6       AO CO -A1 C1 DO A2 C2 D1 -D2
$A       A     C             4       AO CO A1 C1 A2 C2
$9       A       D           4       AO - A1 DO A2 D1 - D2
$8       A                   4       AO - A1 - A2 -
$7       B C D                6       BO CO -B1 -Cl DO B2 C2 D1 -D2
$6       B C                  6       BO CO -B1 Cl - B2 C2
$5       B   D                6       BO -B1 DO - B2 D1 D2
$4       B                    4       BO -B1 - B2
$3       C D                  4       CO -Cl DO - C2 D1 D2
$2       C                    4       CO -Cl C2
$1           D                4       DO -D1 D2
$0       none                 —
```

### Minterm quick-reference

```
Function      LF    | Function     LF
D = 0         $00   | D = ABC      $80
D = A         $F0   | D = AB+A̅C    $CA (cookie cut)
D = ¬A        $0F   | D = ABC̄+A̅C   $4A (cookie cut XOR)
D = B         $CC   | D = AB       $C0
D = C         $AA   | D = AC       $A0
D = A+B       $FC   | D = BC       $88
D = A+C       $FA   | D = ¬A∧¬B    $03
D = B+C       $EE   | D = ¬A∧¬C    $05
D = A⊕B⊕C    $96   | D = 1        $FF
```

### Area fill cheat sheet

- `DESC = 1` required (area fill always walks right-to-left).
- `IFE = 1` → inclusive fill (boundary bits retained).
- `EFE = 1` → exclusive fill (boundary bits consumed).
- `FCI` = 1 → start each row "inside", so result is inverted.
- Must pre-draw outlines with one set bit per horizontal row per edge
  (use line-mode with `SING = 1`).

---

## Appendix C — Color register map

Summary of colour register usage. Register addresses are relative to $DFF000.

| Register | Addr   | Default use |
|----------|--------|-------------|
| COLOR00  | $180   | Background (and dual-PF transparent for PF1) |
| COLOR01  | $182   | PF1 value 001 |
| COLOR02  | $184   | PF1 value 010 |
| COLOR03  | $186   | PF1 value 011 |
| COLOR04  | $188   | PF1 value 100 |
| COLOR05  | $18A   | PF1 value 101 |
| COLOR06  | $18C   | PF1 value 110 |
| COLOR07  | $18E   | PF1 value 111 |
| COLOR08  | $190   | PF2 value 000 → transparent in DPF |
| COLOR09  | $192   | PF2 value 001 |
| COLOR10  | $194   | PF2 value 010 |
| COLOR11  | $196   | PF2 value 011 |
| COLOR12  | $198   | PF2 value 100 |
| COLOR13  | $19A   | PF2 value 101 |
| COLOR14  | $19C   | PF2 value 110 |
| COLOR15  | $19E   | PF2 value 111 |
| COLOR16  | $1A0   | Sprites 0/1 value 00 (unused → transparent) |
| COLOR17  | $1A2   | Sprites 0/1 value 01 |
| COLOR18  | $1A4   | Sprites 0/1 value 10 |
| COLOR19  | $1A6   | Sprites 0/1 value 11 |
| COLOR20  | $1A8   | Sprites 2/3 value 00 (unused) |
| COLOR21  | $1AA   | Sprites 2/3 value 01 |
| COLOR22  | $1AC   | Sprites 2/3 value 10 |
| COLOR23  | $1AE   | Sprites 2/3 value 11 |
| COLOR24  | $1B0   | Sprites 4/5 value 00 (unused) |
| COLOR25  | $1B2   | Sprites 4/5 value 01 |
| COLOR26  | $1B4   | Sprites 4/5 value 10 |
| COLOR27  | $1B6   | Sprites 4/5 value 11 |
| COLOR28  | $1B8   | Sprites 6/7 value 00 (unused) |
| COLOR29  | $1BA   | Sprites 6/7 value 01 |
| COLOR30  | $1BC   | Sprites 6/7 value 10 |
| COLOR31  | $1BE   | Sprites 6/7 value 11 |

Bit layout of each colour register (OCS):

```
Bit  15 14 13 12  11 10  9  8   7  6  5  4   3  2  1  0
     0  0  0  0   R3 R2 R1 R0  G3 G2 G1 G0  B3 B2 B1 B0
```

Write-only on OCS (reads return undefined).

---

## Appendix D — graphics.library function index

One-line descriptions for the graphics.library functions relevant to the display
system (Autodocs, `graphics.library` library). Not exhaustive; focused on the
bitmap and display side.

**Library management**
- `OpenLibrary("graphics.library", 0)` — via exec; returns GfxBase pointer.
- `CloseLibrary(GfxBase)`.

**View / ViewPort / display**
- `InitView(view)` — zero-initialise a View.
- `InitVPort(vp)` — zero-initialise a ViewPort.
- `MakeVPort(view, vp)` — generate preliminary Copper list for a ViewPort.
- `MrgCop(view)` — merge all ViewPort preliminary lists into LOFCprList and
  SHFCprList.
- `LoadView(view)` — install the given View for display (writes COP1LC).
- `ScrollVPort(vp)` — update the ViewPort's Copper list after changing RasInfo
  offsets.
- `FreeVPortCopLists(vp)` — release preliminary Copper lists built by MakeVPort.
- `FreeCprList(cprList)` — release a merged Copper list from MrgCop.

**Timing**
- `WaitTOF()` — wait for the next "top of frame" (vertical blank start).
- `WaitBOVP(vp)` — wait for the bottom of a given ViewPort.
- `WaitBlit()` — wait for the blitter to finish (with the known BBUSY quirk).

**BitMap and raster**
- `InitBitMap(bm, depth, width, height)` — fill in a BitMap structure.
- `AllocRaster(width, height)` — allocate chip-RAM for one bitplane; returns
  `PLANEPTR`.
- `FreeRaster(ptr, width, height)` — release.

**ColorMap**
- `GetColorMap(n)` — allocate a ColorMap with n entries.
- `FreeColorMap(cm)` — release.
- `LoadRGB4(vp, colortable, count)` — copy a UWORD colour table into the ViewPort's
  ColorMap.
- `SetRGB4(vp, index, r, g, b)` — set a single colour, updating the Copper list.
- `GetRGB4(cm, index)` — read a ColorMap entry.

**RastPort**
- `InitRastPort(rp)` — zero-initialise.
- `SetAPen(rp, pen)` / `SetBPen(rp, pen)` / `SetOPen(rp, pen)` — set drawing pens.
- `SetDrMd(rp, mode)` — JAM1 | JAM2 | COMPLEMENT | INVERSEVID.
- `SetDrPt(rp, pattern)` — set line pattern.
- `SetAfPt(rp, pattern, sizeLog2)` — set area pattern.
- `SetRast(rp, pen)` — fill entire raster.
- `SetWrMsk(rp, mask)` — write mask for plane protection.

**Drawing primitives**
- `WritePixel(rp, x, y)` — set one pixel.
- `ReadPixel(rp, x, y)` — returns pen value or -1.
- `Move(rp, x, y)` — move pen position.
- `Draw(rp, x, y)` — draw line from current pen to (x,y).
- `PolyDraw(rp, count, array)` — draw a polyline.
- `RectFill(rp, xmin, ymin, xmax, ymax)` — fill a rectangle.
- `ScrollRaster(rp, dx, dy, xmin, ymin, xmax, ymax)` — shift a region.
- `Flood(rp, mode, x, y)` — flood fill.

**Area fill**
- `InitArea(areaInfo, buffer, maxVerts)` — attach an area buffer to a RastPort.
- `AreaMove(rp, x, y)` — start a subpath.
- `AreaDraw(rp, x, y)` — add a vertex.
- `AreaEnd(rp)` — rasterise the polygon.
- `AreaCircle(rp, cx, cy, r)` — add a circle vertex sequence.
- `AreaEllipse(rp, cx, cy, rx, ry)` — add an ellipse.

**Blitter-wrapped**
- `BltBitMap(srcBM, srcX, srcY, dstBM, dstX, dstY, sizeX, sizeY, minterm, mask, temp)`
  — rectangular bitmap copy with minterm and plane mask.
- `BltBitMapRastPort(srcBM, srcX, srcY, dstRP, dstX, dstY, w, h, minterm)` — copy
  into a RastPort (respects layers).
- `BltClear(ptr, bytes, flags)` — clear memory to zero (blitter-fast).
- `BltPattern(rp, mask, xmin, ymin, xmax, ymax, byteCnt)` — area fill with the
  current area pattern.
- `BltTemplate(source, srcX, srcMod, destRP, destX, destY, sizeX, sizeY)` — cookie
  cut a monochrome template.
- `ClipBlit(srcRP, srcX, srcY, destRP, destX, destY, sizeX, sizeY, minterm)` —
  copy between RastPorts with clipping.

**Blitter arbitration**
- `OwnBlitter()` — take the blitter semaphore (multitasking).
- `DisownBlitter()` — release it.
- `QBlit(bltnode)` — queue a blit.
- `QBSBlit(bltnode)` — queue a beam-sync blit.

**Text**
- `Text(rp, string, length)` — render using current font.
- `TextLength(rp, string, length)` — return width in pixels.
- `SetFont(rp, font)` — set current font.
- `OpenFont(textAttr)` / `CloseFont(font)` — from font list.
- `OpenDiskFont(textAttr)` / `CloseDiskFont(font)` — diskfont.library.

**Animation (VSprites / Bobs)**
- `InitGels(head, tail, gelsInfo)` — set up a GEL list.
- `AddVSprite(vs, rp)` / `RemVSprite(vs)`.
- `AddBob(bob, rp)` / `RemBob(bob)`.
- `DoCollision(rp)` — run collision routines.
- `SortGList(rp)` — sort by display Y.
- `DrawGList(rp, vp)` — draw all GELs.
- `MrgCop(view)` — also merges any GEL-generated Copper instructions.

**Sprite control**
- `GetSprite(simpleSprite, num)` / `FreeSprite(num)` — allocate a hardware sprite.
- `ChangeSprite(vp, simpleSprite, newdata)` — reload sprite data.
- `MoveSprite(vp, simpleSprite, x, y)` — reposition.

---

## Appendix E — intuition.library function index

One-line summaries for the core intuition.library functions (Autodocs).

**Library**
- `OpenLibrary("intuition.library", 33)` / `CloseLibrary()`.

**Screens**
- `OpenScreen(newScreen)` — v1.3 style, returns Screen *.
- `OpenScreenTagList(ns, tags)` / `OpenScreenTags(ns, ...)` — v2+ style.
- `CloseScreen(screen)` — returns BOOL in v2.
- `ShowTitle(screen, show)` — show or hide title bar.
- `MoveScreen(screen, dx, dy)` — reposition.
- `ScreenToFront(screen)` / `ScreenToBack(screen)`.
- `MakeScreen(screen)` — ask graphics to rebuild Copper list.
- `RethinkDisplay()` — ask graphics to MrgCop() + LoadView() the whole display
  after a custom Copper-list change.
- `RemakeDisplay()` — rebuild all screen Copper lists and redisplay.
- `LockPubScreen(name)` / `UnlockPubScreen(name, screen)` — v2+ public screens.
- `OpenWorkBench()` / `CloseWorkBench()`.

**Windows**
- `OpenWindow(newWindow)` — v1.3 style.
- `OpenWindowTagList(nw, tags)` / `OpenWindowTags(nw, ...)` — v2+.
- `CloseWindow(window)`.
- `MoveWindow(w, dx, dy)` / `SizeWindow(w, dx, dy)` — via messages.
- `WindowToFront(w)` / `WindowToBack(w)`.
- `ActivateWindow(w)` — make active, receives ACTIVEWINDOW IDCMP.
- `SetWindowTitles(w, wtitle, stitle)` — set window title and/or screen title.
- `BeginRefresh(w)` / `EndRefresh(w, complete)` — wrap redraw in refresh mode.
- `RefreshGList(gadgets, w, r, numgads)` — repaint gadgets.
- `ReportMouse(flag, w)` — turn on MOUSEMOVE reporting.

**IDCMP**
- `ModifyIDCMP(w, flags)` — change the set of events the window will receive;
  creates or destroys the UserPort/MessagePort as needed.
- The UserPort is a standard Exec MsgPort; read with `GetMsg(w->UserPort)` and
  dispose with `ReplyMsg(msg)`.

**Gadgets**
- `AddGadget(w, gad, position)` — insert into window gadget list.
- `RemoveGadget(w, gad)`.
- `RefreshGadgets(firstGad, w, r)`.
- `OnGadget(gad, w, r)` / `OffGadget(gad, w, r)` — enable / disable.
- `ModifyProp(gad, w, r, flags, hp, vp, hb, vb)` — change proportional gadget
  parameters.
- `NewModifyProp()`.

**Menus**
- `SetMenuStrip(w, menu)` — attach menu chain.
- `ClearMenuStrip(w)` — detach.
- `ItemAddress(strip, menuNum)` — decode MENUPICK code into a MenuItem pointer.
- `OnMenu(w, menuNum)` / `OffMenu(w, menuNum)` — enable / disable.

**Requesters**
- `Request(req, w)` — begin a requester.
- `EndRequest(req, w)` — end it.
- `AutoRequest(w, body, pos, neg, posflags, negflags, width, height)` — simple
  yes/no system requester.
- `BuildSysRequest(w, body, pos, neg, flags, width, height)` /
  `FreeSysRequest(w)`.
- `DisplayAlert(code, msg, height)` — the red "guru meditation" style panic
  alert.
- `InitRequester(req)`.

**Drawing (thin wrappers around graphics.library)**
- `SetWindowTitles(w, wt, st)` — sets and repaints.
- `PrintIText(rp, itext, left, top)` — render an IntuiText linked list.
- `IntuiTextLength(itext)` — measure text width.
- `DrawBorder(rp, border, left, top)` — render a Border linked list.
- `DrawImage(rp, image, left, top)` — render an Image structure.

**Pointer / sprite**
- `SetPointer(w, data, h, w, xoff, yoff)` — custom pointer for a window.
- `ClearPointer(w)` — revert to default.

**Input handling**
- `InitRequester(req)`, and the `IntuiMessage` class/code constants in
  `<intuition/intuition.h>`.

**BOOPSI (v2+)**
- `NewObject(class, classID, tags)` / `DisposeObject(obj)`.
- `GetAttr(obj, attrID, storage)` / `SetAttrs(obj, tags)`.
- `DoMethod(obj, method, args)`.

---

## Gaps in corpus

The following topics are not or only lightly covered by the ten source files:

1. **AGA (A1200/A4000) graphics** — BPLCON3, BPLCON4, 256-colour modes, 24-bit
   palette, wide sprites, SHRES expansions. HRM 3rd ed mentions ECS in appendices
   but the extracted text does not include Appendix C's detailed pages. Any AGA
   work must consult the AGA addendum.
2. **ECS SuperHires and VGA modes** — the corpus references `BEAMCON0`, HTOTAL,
   HSSTOP/HSSTRT/HBSTRT/HBSTOP registers (HRM p.12470) but does not contain the
   detailed programming information.
3. **Copper timing edge cases** — behaviour of COPJMP strobes when written from a
   Copper MOVE mid-line; precise latency of WAIT wake-up; exact slot at which a
   Copper MOVE to a DDF register takes effect.
4. **Denise serialiser pipeline** — exact number of lores-pixel delays between
   bitplane fetch and pixel output beyond the "4.5 colour clocks" figure in HRM
   p.2991; hires/shres pipeline differences.
5. **Blitter's internal pipelining on ECS/AGA** — HRM p.8515 notes the Fat Agnus
   busy-bit fix but does not describe the pipeline diagrams for ECS.
6. **Sprite data DMA microstructure** — which slot SPRxPOS is fetched in relative
   to SPRxDATA/SPRxDATB; sprite DMA contention with 6-bitplane lores.
7. **Collision-detect timing** — exact pipeline stage at which CLXDAT bits are
   latched.
8. **BOOPSI class hierarchy in full** — the 1990 RKM 3rd has chapter 12 on BOOPSI
   but only a few pages of it are extracted in the corpus text.
9. **GadTools library** — extracted text barely mentions it.
10. **Preferences** — colour preferences, overscan preferences, etc. impact the
    default screen mode but are not detailed in the extracts.
11. **AREXX / ASL / Workbench icon internals** — referenced, not detailed.
12. **Intuition's input.device handler chain priorities and the exact IDCMP event
    layout per class** — some events (e.g., IDCMP_CHANGEWINDOW, IDCMP_IDCMPUPDATE)
    are only listed in passing.

---

## Source map

Which file was the primary source for each section:

| Section | Primary source(s) |
|---------|-------------------|
| 1. Display pipeline | HRM Chapter 3 (Playfield Hardware), HRM Chapter 7 (System Control) |
| 2. Playfield modes | HRM Chapter 3, RKM L&D Chapter 1 |
| 3. Sprites | HRM Chapter 4, HRM Chapter 7 (Collision) |
| 4. Copper | HRM Chapter 2 |
| 5. Blitter | HRM Chapter 6 |
| 6. Color | HRM Chapter 3 tail |
| 7. graphics.library | RKM L&D Chapter 1, Autodocs graphics.library |
| 8. intuition.library | RKM 3rd Chapters 2-11, RKM L&D Chapter 2 (Layers for context) |
| 9. layers.library | RKM L&D Chapter 2 |
| 10. Tying it together | Synthesis from all of the above |
| 11. Timing reference | HRM Chapter 3, HRM Chapter 6 |

The HRM 3rd edition (`Amiga_Hardware_Reference_Manual_3rd_edition.txt`) is by far
the most load-bearing single file and should be the starting point for any
hardware-level ambiguity. The Autodocs file is authoritative for function
signatures and side effects. The 1990 RKM 3rd is the most modern Intuition
reference in the corpus and is preferred over the older RKM L&D (1986) for
Intuition v2 material.
