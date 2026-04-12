# Agnus — Beam Counter, DMA Controller, Copper, Blitter

Agnus owns the chip bus. Every colour clock (CCK), it decides who gets the bus:
the CPU, a DMA channel, the copper, or the blitter. This decision is the single
most important thing to get right in an Amiga emulator — if DMA slot allocation
is wrong, everything downstream breaks.

## Beam Counter

Agnus maintains a beam position counter that drives the entire display system:

| Register | Address | Access | Content |
|----------|---------|--------|---------|
| VPOSR | $DFF004 | Read | V8 (bit 0), LOF (bit 15), Agnus ID (bits 14-8) |
| VHPOSR | $DFF006 | Read | V7-V0 (bits 15-8), H8-H1 (bits 7-0) |
| VPOSW | $DFF02A | Write | Force beam position (dangerous) |
| VHPOSW | $DFF02C | Write | Force beam position (dangerous) |

The counter advances one hpos per CCK. At hpos 227 ($E3), it wraps to 0 and
vpos increments. At the last line of the frame (311 for PAL, 261 for NTSC),
vpos wraps to 0.

### Interlace and LOF

When BPLCON0 bit 2 (LACE) is set, the Long Frame (LOF) flag toggles at each
frame start:

- **Long frame (LOF=1):** 313 lines PAL, 263 NTSC — extra line at end
- **Short frame (LOF=0):** 312 lines PAL, 262 NTSC — standard

LOF is exposed in VPOSR bit 15. Software reads it to know which field is
displaying. The vertical blank interrupt fires at the same line regardless of
LOF — only the frame length changes.

### PAL vs NTSC Detection

VPOSR bit 12 indicates the region:
- Bit 12 = 1: PAL (Agnus is wired for PAL crystal)
- Bit 12 = 0: NTSC

This is hardwired — it reflects the crystal, not a software setting. Kickstart
reads this during graphics.library init to set up display timing.

## DMA Slot Allocation

Each of the 227 CCKs per line is assigned to a slot owner. The first ~28 CCKs
are fixed assignments; the rest are variable (bitplane, copper, or CPU).

### Fixed Slots (CCK $01–$1B)

```
CCK $01-$03:  Memory refresh (3 slots)
CCK $04-$06:  Disk DMA (3 slots, if DSKEN set)
CCK $07:      Audio channel 0 (if AUD0EN set)
CCK $08:      Audio channel 1 (if AUD1EN set)
CCK $09:      Audio channel 2 (if AUD2EN set)
CCK $0A:      Audio channel 3 (if AUD3EN set)
CCK $0B-$1A:  Sprite DMA (8 sprites × 2 slots each, if SPREN set)
CCK $1B:      Memory refresh (1 slot)
```

When a DMA channel is disabled, its slot becomes a CPU slot.

### Sprite DMA Detail

Each sprite uses two consecutive CCKs:
```
CCK $0B-$0C: Sprite 0 (position word, then data word)
CCK $0D-$0E: Sprite 1
CCK $0F-$10: Sprite 2
CCK $11-$12: Sprite 3
CCK $13-$14: Sprite 4
CCK $15-$16: Sprite 5
CCK $17-$18: Sprite 6
CCK $19-$1A: Sprite 7
```

The first CCK fetches SPRxPOS/SPRxCTL (position/control), the second fetches
SPRxDATA/SPRxDATB (image data). Sprite DMA only occurs on lines where the
sprite is active (between VSTART and VSTOP from SPRxCTL).

### Variable Slots (CCK $1C–$E2)

These slots are shared between bitplane DMA, copper, and CPU:

1. **Bitplane DMA** has highest priority. When inside the data fetch window
   (DDFSTRT to DDFSTOP), bitplane slots consume CCKs according to the fetch
   order table.
2. **Copper** takes even-numbered CCKs that aren't used by bitplanes.
3. **CPU** gets whatever is left.

### Bitplane Fetch Order

Bitplanes are fetched in a specific interleaved order within repeating groups.
The last plane fetched (BPL1) triggers the shift register load in Denise.

**Lowres (8-CCK groups):**

| Position in group | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|-------------------|---|---|---|---|---|---|---|---|
| Plane fetched | free | BPL4 | BPL6 | BPL2 | free | BPL3 | BPL5 | BPL1 |

**Hires (4-CCK groups):**

| Position in group | 0 | 1 | 2 | 3 |
|-------------------|---|---|---|---|
| Plane fetched | BPL4 | BPL2 | BPL3 | BPL1 |

Slots for planes beyond the current depth (BPLCON0 BPU field) are free and
available to copper or CPU. A 2-plane lowres display uses only positions 3 and
7, leaving 6 of 8 slots free.

**AGA extensions:** Planes 7 and 8 (BPL7/BPL8) use the two free slots in
lowres position 0 and 4. This means a full 8-plane AGA lowres display has zero
free slots per group — the copper gets no time during the active display area.

### Data Fetch Window

DDFSTRT ($DFF092) and DDFSTOP ($DFF094) define where bitplane DMA starts and
stops on each line. The fetch runs in 8-CCK "fetch unit" blocks:

1. Fetch starts at DDFSTRT (should be aligned to fetch unit boundary)
2. Fetch runs in complete 8-CCK blocks
3. When DDFSTOP is reached, the current block finishes
4. One additional complete block runs after DDFSTOP (hardware pipeline flush)
5. After each block, bitplane pointers advance by adding BPLxMOD

Standard values:
- Lowres: DDFSTRT=$0038, DDFSTOP=$00D0 (normal display)
- Hires: DDFSTRT=$003C, DDFSTOP=$00D4

### Display Window

DIWSTRT ($DFF08E) and DIWSTOP ($DFF090) define the visible window. This is
independent of the data fetch window — DDFSTRT/DDFSTOP controls when data is
fetched, DIWSTRT/DIWSTOP controls when it is displayed.

- Bits 15-8: vertical start/stop (V7-V0)
- Bits 7-0: horizontal start/stop (H8-H1, in lowres pixels)
- V8 and H8 are implicit (start is assumed < 256, stop > 256)

ECS adds DIWHIGH ($DFF1E4) for explicit V8/H8 control, enabling vertical
windows that cross the V8 boundary.

### CPU Bus Access

The CPU operates at half the CCK rate (one CPU cycle = 2 CCKs). When the CPU
needs the chip bus (to read/write chip RAM or custom registers), it must wait
for a CCK where no DMA channel has priority.

In the worst case (6-plane lowres with sprites and disk active), the CPU can
lose most of its chip-bus bandwidth. This is the source of "DMA stealing" that
slows down Amiga programs during complex displays.

The blitter adds another dimension: when BLTPRI (blitter nasty mode) is set in
DMACON, the blitter takes all free slots, starving the CPU entirely until the
blit completes.

## Copper

The copper is a simple coprocessor that writes to custom chip registers
synchronised to the beam position. It has three instructions: MOVE, WAIT, and
SKIP.

### Instruction Format

All instructions are two 16-bit words:

**MOVE** (bit 0 of first word = 0):
```
Word 1:  [register offset, 9 bits]  [0]
Word 2:  [value to write, 16 bits]
```

**WAIT** (bit 0 of first word = 1, bit 0 of second word = 0):
```
Word 1:  [VP, 8 bits]  [HP, 7 bits]  [1]
Word 2:  [VM, 7 bits]  [HM, 7 bits]  [0]  (bit 15 = BFD, blitter-finished-disable)
```

**SKIP** (bit 0 of first word = 1, bit 0 of second word = 1):
```
Word 1:  [VP, 8 bits]  [HP, 7 bits]  [1]
Word 2:  [VM, 7 bits]  [HM, 7 bits]  [1]
```

### Execution Timing

The copper takes 2 CCKs per instruction:
- CCK 1: Fetch first word into IR1
- CCK 2: Fetch second word into IR2, then execute

For MOVE: the register write happens on the second CCK. For WAIT: the copper
enters Wait state and checks the beam position on each subsequent copper slot.

**Copper slots:** The copper runs on even-numbered CCKs in the variable region
($1C–$E2) that aren't taken by bitplane DMA. It never runs on odd CCKs.

### WAIT Comparison

The beam comparison uses masked fields:

```
current_v = VPOS & (mask_v | 0x80)    — V7 is always compared
current_h = (HPOS >> 1) & mask_h
wait_v = VP & (mask_v | 0x80)
wait_h = HP & mask_h

Resolves when: (current_v, current_h) >= (wait_v, wait_h)
```

**V7 always compared:** Bit 7 of the vertical position is always part of the
comparison, even though the mask register has no V7 bit. Without this, a WAIT
for line $F4 would falsely trigger at line $74. This is a common emulator bug.

### End-of-List

The conventional end-of-list marker is `$FFFF,$FFFE` — a WAIT for position
$FF,$7F with full mask. Since V7 is always compared and the beam never reaches
vertical position $FF with H=$7F simultaneously, this never resolves. The
copper stays in Wait state until the next vertical blank restarts it.

### Copper Lists and Restart

Two copper list pointers exist:
- COP1LC ($DFF080/082): Primary list, restarted at every vertical blank
- COP2LC ($DFF084/086): Secondary list, used by software (COPJMP2 strobe)

COPJMP1 ($DFF088, write-only strobe): Restart copper from COP1LC
COPJMP2 ($DFF08A, write-only strobe): Restart copper from COP2LC

At vertical blank, Agnus automatically triggers COPJMP1.

### Danger Bit (COPCON)

COPCON ($DFF02E) bit 1 (CDANG) controls whether the copper can write to
registers below $040. With CDANG=0, writes to low registers (including
DMACON, INTENA, INTREQ) are blocked. This prevents copper list corruption
from triggering dangerous state changes. KS sets CDANG during boot.

## Blitter

The blitter performs bulk memory operations: area copies with logic, line
drawing, and area fills. It operates through Agnus's DMA system, using chip-bus
slots that would otherwise go to the CPU.

### Channels

Four channels, each with a pointer, modulo, and data register:

| Channel | Purpose | Registers |
|---------|---------|-----------|
| A | Source with barrel shift + masks | BLTAPT, BLTAMOD, BLTADAT, BLTAFWM, BLTALWM |
| B | Source with barrel shift | BLTBPT, BLTBMOD, BLTBDAT |
| C | Source (no shift) | BLTCPT, BLTCMOD, BLTCDAT |
| D | Destination (write-only) | BLTDPT, BLTDMOD |

### Area Mode

Triggered by writing BLTSIZE ($DFF058). The blitter processes a rectangular
area word-by-word:

1. For each word position in the row:
   a. Read enabled source channels (A, B, C) — one DMA slot each
   b. Apply barrel shift to A and B (BLTCON0 bits 15-12 for A, BLTCON1 bits
      15-12 for B)
   c. Apply first/last word masks to A (BLTAFWM on first word, BLTALWM on last)
   d. Apply minterm logic function (BLTCON0 bits 7-0): D = f(A, B, C)
   e. Write D — one DMA slot
2. At end of row, add modulo to each pointer
3. Repeat for all rows

Each enabled channel costs one DMA slot per word. A full A+B+C→D blit uses 4
slots per word. An A→D copy uses 2 slots per word. The CPU is locked out of
the chip bus for the duration (in nasty mode) or gets alternating slots.

**Descending mode:** BLTCON1 bit 1 (DESC) reverses the direction. Pointers
start at the bottom-right and step backward. Used for overlapping copies where
source and destination overlap with destination at a lower address.

### Line Mode

BLTCON1 bit 0 (LINE) enables Bresenham line drawing:

- Channel C reads the destination word (read-modify-write)
- Channel D writes the modified word
- Channel A provides a single-pixel mask (one bit set)
- BLTCON0 bits 15-12 encode the pixel position within the word
- BLTCON1 bits 4-2 encode the octant

The blitter steps one pixel per iteration, using channels C and D for
read-modify-write. Texture from BLTBDAT can modulate the line pattern.

Line mode uses a different per-step timing than area mode — each step needs
a C-read and D-write (2 slots minimum), plus internal error-accumulator cycles.

### Fill Mode

BLTCON1 bit 3 (EFE = exclusive fill) or bit 4 (IFE = inclusive fill) enables
area fill during an area blit:

- A fill carry propagates right-to-left across each word
- Inclusive fill: set pixel when carry is active, toggle carry on each set pixel
- Exclusive fill: toggle pixel when carry toggles

Fill is applied after the minterm logic and before writing D. It operates
within a single row — carry resets at each new row.

### Blitter-Finished Interrupt

When the blitter completes an operation, it sets INTREQ bit 6 (BLIT). Software
polls DMACON bit 14 (BBUSY) or waits for the interrupt.

### ECS Extended Size

OCS BLTSIZE encodes height (bits 15-6, max 1024) and width (bits 5-0, max 64
words). ECS adds BLTSIZV ($DFF05C) and BLTSIZH ($DFF05E) for up to 32768 ×
32768 sizes. Writing BLTSIZH triggers the blit (like BLTSIZE does for OCS).

## DMACON

DMACON ($DFF096 write, $DFF002 read) controls all DMA channels:

| Bit | Name | Channel |
|-----|------|---------|
| 15 | SET/CLR | 1 = set bits, 0 = clear bits (write only) |
| 14 | BBUSY | Blitter busy (read only) |
| 13 | BZERO | Blitter zero flag (read only) |
| 9 | DMAEN | Master DMA enable — all channels gated by this |
| 8 | BPLEN | Bitplane DMA |
| 7 | COPEN | Copper DMA |
| 6 | BLTEN | Blitter DMA |
| 5 | SPREN | Sprite DMA |
| 4 | DSKEN | Disk DMA |
| 3 | AUD3EN | Audio channel 3 |
| 2 | AUD2EN | Audio channel 2 |
| 1 | AUD1EN | Audio channel 1 |
| 0 | AUD0EN | Audio channel 0 |

**SET/CLR semantics:** Writing $8200 sets bit 9 (DMAEN) without changing other
bits. Writing $7FFF clears all bits (bit 15 = 0 means "clear the bits that are
1 in the value"). Reading always returns the current state with bit 15 = 0.

**Master enable:** Every DMA channel is gated by DMAEN (bit 9) AND its own
enable bit. Writing $8001 (set AUD0EN) does nothing unless DMAEN is also set.
