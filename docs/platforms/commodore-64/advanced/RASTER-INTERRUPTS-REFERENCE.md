# Raster Interrupts Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

The VIC-II chip provides a powerful interrupt system that can trigger at specific raster positions (scan lines) on the screen. This allows programs to change graphics modes, colors, sprite positions, and other settings **mid-screen**, enabling advanced visual effects.

**Key capabilities:**
- Split-screen modes (bitmap + text, different modes per section)
- More than 8 sprites on screen (sprite multiplexing)
- Color changes mid-screen
- Status bar areas with different graphics modes
- Smooth scrolling with minimal CPU overhead

**Note:** While BASIC can set up basic raster interrupts, most practical uses require machine language for speed.

---

## Raster Register Basics

### Raster Position Register

**Location:** 53266 ($D012) - Read/Write register

**Reading the register:**
- Returns the lower 8 bits of the current raster beam position (0-255)
- Bit 7 of register 53265 ($D011) provides the 9th bit (MSB)
- Combined value gives raster line 0-312 (NTSC) or 0-311 (PAL)

**Writing to the register:**
- Sets the raster compare value (target line for interrupt)
- When actual raster position matches this value, interrupt can fire
- Must also write MSB to register 53265 bit 7 for lines > 255

### Raster Position Formula

```basic
REM Read current raster position (full 9-bit value)
LOW = PEEK(53266)  : REM Lower 8 bits
HIGH = (PEEK(53265) AND 128) / 128  : REM MSB (bit 7)
RASTER = LOW + (HIGH * 256)
```

---

## Visible Display Area

### Screen Raster Ranges

**Visible area (where graphics appear):**
- Raster lines: **51 to 251** (approximately)
- Changes in this range may cause visible flicker
- Updates should occur **outside** this range when possible

**Safe update zones:**
- Top border: Lines 0-50
- Bottom border: Lines 252-312 (NTSC) or 252-311 (PAL)

**Recommendation:** Make screen changes when raster is **not** in the visible display area (51-251) to avoid flicker.

---

## Interrupt Status Register

**Location:** 53273 ($D019)

### Status Bits

| Bit | Name | Description |
|-----|------|-------------|
| 0 | IRST | Set when current raster = compare value |
| 1 | IMDC | Set by sprite-data collision (first only, until reset) |
| 2 | IMMC | Set by sprite-sprite collision (first only, until reset) |
| 3 | ILP | Set by light pen negative transition (1 per frame) |
| 7 | IRQ | Set when any enabled interrupt occurs |

**Reading the register:**
```basic
REM Check which interrupt occurred
STATUS = PEEK(53273)
IF STATUS AND 1 THEN PRINT "RASTER INTERRUPT"
IF STATUS AND 2 THEN PRINT "SPRITE-DATA COLLISION"
IF STATUS AND 4 THEN PRINT "SPRITE-SPRITE COLLISION"
IF STATUS AND 8 THEN PRINT "LIGHT PEN"
```

**Clearing interrupt flags:**
- Bits remain set ("latched") until explicitly cleared
- Clear by writing a 1 to the specific bit
- Allows selective interrupt handling

```basic
REM Clear raster interrupt flag
POKE 53273, 1

REM Clear sprite-sprite collision flag
POKE 53273, 4

REM Clear all interrupt flags
POKE 53273, 255
```

**Important:** Always clear the interrupt flag in your interrupt handler, or the interrupt will not fire again.

---

## Interrupt Enable Register

**Location:** 53274 ($D01A)

### Enable Bits

Same format as status register. Each bit enables the corresponding interrupt source:

| Bit | Interrupt Source |
|-----|-----------------|
| 0 | Raster compare interrupt |
| 1 | Sprite-data collision interrupt |
| 2 | Sprite-sprite collision interrupt |
| 3 | Light pen interrupt |

**Enabling interrupts:**
```basic
REM Enable raster interrupt
POKE 53274, 1

REM Enable sprite-sprite collision interrupt
POKE 53274, 4

REM Enable multiple interrupts (raster + collisions)
POKE 53274, 7  : REM 7 = binary 00000111 = bits 0, 1, 2
```

**Disabling interrupts:**
```basic
REM Disable all VIC-II interrupts
POKE 53274, 0
```

**Note:** The interrupt status register can still be polled even when interrupts are disabled. The bits will be set, but no IRQ will occur.

---

## Setting Up Raster Interrupts

### Basic Setup Sequence

```basic
REM 1. Set raster compare value (line 100)
POKE 53266, 100

REM 2. Clear bit 7 for lines 0-255 (not needed for line 100)
POKE 53265, PEEK(53265) AND 127

REM 3. Clear any pending interrupt
POKE 53273, 1

REM 4. Enable raster interrupt
POKE 53274, 1
```

### Setting Raster Lines > 255

For raster lines 256-312, must set bit 7 of register 53265:

```basic
REM Set raster interrupt for line 280
POKE 53266, 24  : REM Low 8 bits of 280 = 24 (280-256=24)
POKE 53265, PEEK(53265) OR 128  : REM Set MSB (bit 7)
POKE 53273, 1  : REM Clear interrupt
POKE 53274, 1  : REM Enable interrupt
```

---

## Split Screen Techniques

### Concept

By using raster interrupts, you can change VIC-II settings at specific scan lines, creating different graphics modes or settings on different parts of the screen.

### Example: Bitmap Top, Text Bottom

**Goal:** Top half of screen in bitmap mode, bottom half in text mode

**Setup:**
1. Set raster interrupt for middle of screen (line 125)
2. When interrupt fires: Switch to text mode
3. Set second interrupt for top of screen (line 0)
4. When second interrupt fires: Switch back to bitmap mode

**BASIC Example (conceptual - machine language recommended):**
```basic
REM Initial setup - bitmap mode
POKE 53265, PEEK(53265) OR 32  : REM Enable bitmap
POKE 53266, 125  : REM Interrupt at line 125
POKE 53273, 1  : REM Clear interrupt
POKE 53274, 1  : REM Enable interrupt

REM In interrupt handler (machine language required):
REM At line 125: POKE 53265, PEEK(53265) AND 223 (turn off bitmap)
REM Set next interrupt for line 0
REM At line 0: POKE 53265, PEEK(53265) OR 32 (turn on bitmap)
REM Set next interrupt for line 125
```

### Example: More Than 8 Sprites

**Sprite multiplexing:** Reuse sprite numbers at different vertical positions

**Technique:**
1. Display sprites 0-7 at top of screen
2. Set raster interrupt for middle of screen
3. When interrupt fires: Move sprites 0-7 to new Y positions
4. Effectively displays 16+ sprites (8 on top, 8 on bottom)

```basic
REM Setup (sprites 0-7 at top)
FOR I = 0 TO 7
  POKE 53248 + (I * 2), 50 + (I * 40)  : REM X positions
  POKE 53249 + (I * 2), 80  : REM Y position (top)
NEXT I
POKE 53269, 255  : REM Enable all sprites

REM Interrupt at line 150
POKE 53266, 150
POKE 53273, 1
POKE 53274, 1

REM In interrupt handler:
REM Move sprites to bottom half
FOR I = 0 TO 7
  POKE 53249 + (I * 2), 180  : REM New Y position (bottom)
NEXT I
```

**Note:** BASIC is too slow for smooth sprite multiplexing. Machine language required for practical use.

---

## Polling vs Interrupts

### Polling Method

Check the interrupt status register in a loop:

```basic
REM Wait for raster line 100
10 IF PEEK(53266) < > 100 THEN GOTO 10
20 REM Raster is at line 100, make changes
30 POKE 53281, 2  : REM Change background to red
```

**Advantages:**
- Simple to implement in BASIC
- No interrupt setup required
- Good for learning

**Disadvantages:**
- Wastes CPU time in the loop
- Can't do other work while waiting
- Timing can be imprecise

### Interrupt Method

Let the hardware notify you when raster reaches target:

```basic
REM Enable interrupt, then continue with other work
POKE 53266, 100  : REM Set target line
POKE 53273, 1  : REM Clear interrupt
POKE 53274, 1  : REM Enable interrupt

REM Program continues, interrupt will fire at line 100
REM (Requires machine language interrupt handler)
```

**Advantages:**
- CPU free to do other work
- Precise timing
- Multiple interrupts possible
- Professional technique

**Disadvantages:**
- Requires machine language for practical use
- More complex setup
- Must manage interrupt handlers

---

## Important Notes

### BASIC Limitations

**BASIC is too slow** for most practical raster interrupt uses:
- Interrupt latency is high
- Handler execution is slow
- Can't make multiple rapid changes

**Machine language required for:**
- Sprite multiplexing
- Smooth split-screen effects
- Complex color changes
- Professional-quality effects

### Interrupt Handler Requirements

**Must do quickly:**
1. Determine which interrupt occurred (read 53273)
2. Perform required action (change mode, move sprites, etc.)
3. Clear interrupt flag (write to 53273)
4. Set up next interrupt if needed
5. Return from interrupt (RTI instruction in machine language)

**Timing critical:**
- Handler must complete before next interrupt
- Slow handlers cause missed interrupts
- Keep handlers as short as possible

### Avoiding Conflicts

**Don't interfere with:**
- System timer interrupts (used by KERNAL)
- Keyboard scanning
- Serial port communication

**Solution:** Chain interrupts - call original handler after your code, or use dedicated interrupt vector.

---

## Advanced Techniques

### Dynamic Raster Interrupt Chains

Set up multiple interrupts for different screen sections:

```
Line 0:   Switch to mode A, set next interrupt to line 100
Line 100: Switch to mode B, set next interrupt to line 200
Line 200: Switch to mode C, set next interrupt to line 0
```

### Color Bar Effects

Change background color every few raster lines:

```
Line 50:  Background = red, next interrupt 60
Line 60:  Background = orange, next interrupt 70
Line 70:  Background = yellow, next interrupt 80
...
```

### Scrolling Status Bar

Keep status bar stationary while main screen scrolls:

```
Main screen: Scrolling game area
Interrupt at status bar line: Reset scroll registers, display fixed text
Interrupt at end of status: Restore scroll registers
```

---

## Register Quick Reference

| Register | Decimal | Hex | Purpose |
|----------|---------|-----|---------|
| 53265 | $D011 | Raster MSB (bit 7), screen control |
| 53266 | $D012 | Raster compare/position (lower 8 bits) |
| 53273 | $D019 | Interrupt status (read) / clear (write) |
| 53274 | $D01A | Interrupt enable |

---

## Troubleshooting

### Interrupt Not Firing

**Causes:**
1. Interrupt not enabled (check 53274)
2. Interrupt flag not cleared from previous interrupt
3. Raster line value incorrect
4. CIA timer interrupts blocking VIC-II interrupts

**Solutions:**
```basic
POKE 53274, 1  : REM Ensure enabled
POKE 53273, 1  : REM Clear any pending interrupt
REM Verify raster line is correct for your system (NTSC/PAL)
```

### Screen Flicker

**Cause:** Making changes during visible display area

**Solution:** Set interrupt for border area (lines 0-50 or 252+)

### Missed Interrupts

**Cause:** Interrupt handler too slow or interrupts firing too quickly

**Solution:**
- Optimize handler code (use machine language)
- Increase spacing between interrupts
- Simplify operations performed in handler

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for graphics modes
- **Related:** See SPRITES-REFERENCE.md for sprite control

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
