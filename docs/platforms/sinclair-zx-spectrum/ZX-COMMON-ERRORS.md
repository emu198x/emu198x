# ZX Spectrum Common Errors and Pitfalls

**Purpose:** Document common mistakes in ZX Spectrum BASIC programming and how to avoid them
**Audience:** ZX Spectrum curriculum designers and BASIC programmers
**Last Updated:** 2025-10-30

---

## Critical Hardware Quirks

### 1. Screen Memory Thirds Organization

**Problem:** Screen bitmap is NOT linear. Consecutive screen rows are NOT consecutive in memory.

**Symptoms:**
- Pixel appears in wrong location
- Graphics corruption
- "Striped" appearance instead of solid shapes

**Wrong (Linear Calculation):**
```basic
10 REM WRONG - Linear address calculation
20 LET y=100
30 LET x=128
40 LET addr=16384+(y*32)+(x/8)  : REM WRONG!
50 POKE addr,255
```

**Correct (Thirds Formula):**
```basic
10 REM CORRECT - Thirds formula
20 LET y=100
30 LET x=128
40 LET y3=(y AND 7)*32+(y AND 56)*4+(y AND 192)*32
50 LET addr=16384+y3+INT (x/8)
60 POKE addr,255
```

**Explanation:** The screen is divided into three 2KB sections (thirds). Within each third, scanlines are interleaved. This was done for hardware efficiency but makes pixel plotting complex.

**Memory Organization:**
```
Top third    ($4000-$47FF): Rows 0-63
Middle third ($4800-$4FFF): Rows 64-127
Bottom third ($5000-$57FF): Rows 128-191
```

**Rule:** ALWAYS use the thirds formula for direct screen bitmap access. Or use PLOT command which handles this automatically.

---

### 2. Colour Clash (Attribute System)

**Problem:** Each 8×8 pixel cell can only have ONE INK colour and ONE PAPER colour.

**Symptoms:**
- Cannot have multiple colours within 8×8 block
- Colours "spread" to adjacent pixels
- Graphics look blocky with limited colours

**Example of Clash:**
```basic
10 REM Trying to make a multi-colour sprite
20 INK 2: PAPER 0
30 PLOT 64,80  : REM Red pixel

40 INK 4: PAPER 0
50 PLOT 68,80  : REM Green pixel in SAME 8×8 cell

60 REM Result: BOTH pixels become green (last INK set)
70 REM This is "colour clash"
```

**Correct Understanding:**
```basic
10 REM Design graphics within 8×8 attribute boundaries
20 REM Each 8×8 block = 1 INK + 1 PAPER only

30 REM Set attribute for block
40 INK 2: PAPER 0

50 REM Draw within that block
60 FOR x=64 TO 71
70   FOR y=80 TO 87
80     PLOT x,y
90   NEXT y
100 NEXT x

110 REM Want different colour? Use DIFFERENT 8×8 block
120 INK 4: PAPER 0
130 FOR x=72 TO 79  : REM Next cell over
140   FOR y=80 TO 87
150     PLOT x,y
160   NEXT y
170 NEXT x
```

**Workarounds:**
- Design graphics on 8×8 grid boundaries
- Use BRIGHT attribute for some variation
- Accept the limitation (all Spectrum games have this)
- Use more blocks for multi-coloured objects

**Rule:** Each 8×8 character cell = 1 INK + 1 PAPER. This is a hardware limitation, not a bug.

---

### 3. PRINT AT vs PLOT Coordinate Systems

**Problem:** PRINT AT and PLOT use DIFFERENT coordinate systems and origins.

**PRINT AT:**
- Row, Column (row first!)
- Rows: 0-21 (22 rows)
- Columns: 0-31 (32 columns)
- Origin: Top-left (0,0)

**PLOT:**
- X, Y (x first!)
- X: 0-255 (256 pixels)
- Y: 0-175 (176 pixels)
- Origin: Bottom-left (0,0)

**Wrong (Confusing the two):**
```basic
10 PRINT AT 10,10; "X"  : REM Row 10, Col 10
20 PLOT 10,10            : REM DIFFERENT LOCATION!
30 REM PLOT is X=10 (left), Y=10 (near bottom)
```

**Correct (Understanding the difference):**
```basic
10 REM Text at character position row 10, col 10
20 PRINT AT 10,10; "X"

30 REM Pixel near same location
40 REM Row 10 in PRINT = Y around 80-87 in PLOT
50 REM Col 10 in PRINT = X around 80-87 in PLOT
60 PLOT 85,85  : REM Roughly middle of character cell

70 REM Conversion formula:
80 REM PRINT AT row, col → PLOT x, y
90 REM x = col * 8 + 4 (middle of cell)
100 REM y = 175 - (row * 8 + 4) (invert Y axis)
```

**Rule:** PRINT AT uses character cells (row, col). PLOT uses pixels (x, y) with different origin.

---

### 4. Attributes Are Linear, Bitmap Is Not

**Problem:** Attribute memory (colour) is linear, but bitmap memory (pixels) uses thirds.

**Wrong (Using thirds formula for attributes):**
```basic
10 REM WRONG - Using thirds for attributes
20 LET row=10: LET col=15
30 LET y3=(row AND 7)*32+(row AND 56)*4+(row AND 192)*32
40 LET attr=22528+y3+col  : REM WRONG!
50 POKE attr,71  : REM Wrong address!
```

**Correct (Linear for attributes):**
```basic
10 REM CORRECT - Attributes are linear
20 LET row=10: LET col=15
30 LET attr=22528+(row*32)+col  : REM Simple linear
40 POKE attr,71  : REM INK 7, PAPER 0, BRIGHT 1
```

**Rule:** Bitmap ($4000-$57FF) = thirds formula. Attributes ($5800-$5AFF) = linear.

---

## BASIC Language Gotchas

### 5. INKEY$ Returns Empty String When No Key Pressed

**Problem:** INKEY$ returns `""` (empty string) if no key is pressed. Must check for empty before comparing.

**Wrong:**
```basic
10 REM Game loop - WRONG
20 IF INKEY$="q" THEN PRINT "UP"
30 IF INKEY$="a" THEN PRINT "DOWN"
40 GO TO 20

50 REM Problem: INKEY$ is called twice per loop
60 REM Second call might get different result
70 REM Also: doesn't check for empty string
```

**Correct:**
```basic
10 REM Game loop - CORRECT
20 LET key$=INKEY$  : REM Store once
30 IF key$<>"" THEN GO SUB 100  : REM Check not empty
40 GO TO 20

100 REM Handle key press
110 IF key$="q" THEN PRINT "UP"
120 IF key$="a" THEN PRINT "DOWN"
130 RETURN
```

**Best Practice:**
```basic
10 REM Robust input handling
20 LET key$=INKEY$
30 IF key$="" THEN GO TO 20  : REM Wait for key
40 REM Now key$ contains a character
50 IF key$="q" THEN PRINT "QUIT"
60 GO TO 20
```

**Rule:** Always store INKEY$ in a variable and check for `""` before comparison.

---

### 6. FOR...NEXT Variable Scope

**Problem:** FOR loop variable persists after loop and holds last value.

**Unexpected Behavior:**
```basic
10 FOR i=1 TO 10
20   PRINT i
30 NEXT i
40 PRINT i  : REM i = 11 (not 10!)
```

**Why:** After `NEXT i`, the loop increments `i` to 11, sees it exceeds 10, and exits. Variable `i` retains value 11.

**Correct Understanding:**
```basic
10 FOR i=1 TO 10
20   PRINT i
30 NEXT i
40 REM i now equals 11 (loop exit value)
50 REM If you need 10, save it: LET last=10
```

**Rule:** FOR loop variable equals (end value + step) after loop exits.

---

### 7. String Slicing Syntax

**Problem:** ZX Spectrum uses `TO` keyword for string slicing, not colons or other syntax.

**Wrong (Other BASIC dialects):**
```basic
10 LET name$="SPECTRUM"
20 PRINT name$(1:4)  : REM WRONG - not ZX Spectrum syntax
```

**Correct (ZX Spectrum):**
```basic
10 LET name$="SPECTRUM"
20 PRINT name$(1 TO 4)  : REM "SPEC"
30 PRINT name$(5 TO )   : REM "TRUM" (to end)
40 PRINT name$( TO 4)   : REM "SPEC" (from start)
```

**Rule:** Use `TO` keyword in string slicing, not colons or dots.

---

### 8. BRIGHT vs FLASH Values

**Problem:** BRIGHT and FLASH only accept 0 or 1, not colour numbers.

**Wrong:**
```basic
10 BRIGHT 7  : REM ERROR - only 0 or 1 allowed
20 FLASH 2   : REM ERROR - only 0 or 1 allowed
```

**Correct:**
```basic
10 BRIGHT 0  : REM Normal intensity
20 BRIGHT 1  : REM Bright intensity

30 FLASH 0   : REM Steady
40 FLASH 1   : REM Flashing (16 frames cycle)
```

**Rule:** BRIGHT and FLASH are binary flags (0 or 1), not colour values.

---

## Memory and Performance

### 9. FRAMES Counter Wraparound

**Problem:** FRAMES counter at address 23672 is 3 bytes and wraps around.

**Wrong (Assuming no wraparound):**
```basic
10 LET start=PEEK 23672
20 REM ... long delay ...
30 LET elapsed=PEEK 23672-start
40 REM If counter wrapped, elapsed is NEGATIVE!
```

**Correct (Handle wraparound):**
```basic
10 REM Read full 3-byte FRAMES counter
20 LET f=PEEK 23672+256*PEEK 23673+65536*PEEK 23674

30 REM Or use PAUSE for fixed delays
40 PAUSE 50  : REM Wait 1 second (50 frames at 50Hz)

50 REM For timing, handle wraparound
60 LET start=PEEK 23672
70 REM ... code ...
80 LET elapsed=PEEK 23672-start
90 IF elapsed<0 THEN LET elapsed=elapsed+256  : REM Wrapped
```

**Rule:** FRAMES (23672) is 1 byte that wraps at 256. Use PAUSE for delays or read full 3-byte counter.

---

### 10. Arrays Auto-DIM to Size 10

**Problem:** ZX Spectrum auto-creates arrays of size 10 if not DIM'd. This wastes memory and can cause confusion.

**Hidden Behavior:**
```basic
10 LET scores(5)=100  : REM No DIM statement
20 REM ZX Spectrum automatically: DIM scores(10)
30 REM Wastes memory if you don't need 11 elements (0-10)
```

**Correct (Explicit DIM):**
```basic
10 DIM scores(20)  : REM Explicit size
20 LET scores(5)=100
30 REM Clear: array has 21 elements (0-20)
```

**Rule:** Always DIM arrays explicitly to avoid auto-sizing to 10.

---

## Graphics and Display

### 11. OVER 1 (XOR Drawing)

**Problem:** OVER 1 mode uses XOR plotting. Drawing same shape twice erases it.

**Behavior:**
```basic
10 OVER 1
20 CIRCLE 128,88,50  : REM Draw circle
30 CIRCLE 128,88,50  : REM XOR again - ERASES circle!

40 REM Useful for sprites (draw, move, erase)
50 OVER 1
60 CIRCLE 100,88,10  : REM Draw
70 PAUSE 10
80 CIRCLE 100,88,10  : REM Erase (XOR same location)
90 CIRCLE 110,88,10  : REM Draw at new position
```

**Rule:** OVER 1 = XOR mode. Drawing twice in same location erases. Useful for animation.

---

### 12. PLOT Moves Graphics Cursor

**Problem:** PLOT sets the graphics cursor position. Subsequent DRAW commands are relative to this.

**Unexpected:**
```basic
10 PLOT 128,88  : REM Sets cursor to (128,88)
20 DRAW 50,0    : REM Draws to (178,88) - relative!
30 DRAW 0,50    : REM Draws to (178,138) - still relative!
```

**Correct Understanding:**
```basic
10 REM PLOT sets absolute position
20 PLOT 100,88

30 REM DRAW is relative to last position
40 DRAW 50,0  : REM Line from (100,88) to (150,88)

50 REM Draw continues from end of last line
60 DRAW 0,50  : REM Line from (150,88) to (150,138)

70 REM To draw disconnected lines, PLOT each start point
80 PLOT 200,100
90 DRAW 30,30
```

**Rule:** PLOT = absolute. DRAW = relative to last graphics cursor position.

---

### 13. Border Colour Not in Attributes

**Problem:** BORDER colour is separate from screen attributes. Cannot be controlled per-character.

**Wrong Assumption:**
```basic
10 REM Trying to change border per 8×8 cell
20 PRINT AT 10,10;  : REM Set position
30 BORDER 2          : REM Entire border becomes red
40 REM Cannot have different border per character cell
```

**Correct Understanding:**
```basic
10 REM BORDER affects entire screen border
20 BORDER 0  : REM Black border (common)
30 BORDER 2  : REM Red border (entire screen)

40 REM Border is single global colour
50 REM Not part of 8×8 attribute system
```

**Rule:** BORDER sets one colour for entire border. Not part of per-cell attributes.

---

## Common BASIC Errors

### 14. Missing STOP Before DATA

**Problem:** Program execution "falls through" into DATA statements, causing errors.

**Wrong:**
```basic
100 PRINT "END OF PROGRAM"
110 DATA 1,2,3,4,5  : REM Program tries to execute this!
120 DATA 6,7,8,9,10
```

**Correct:**
```basic
100 PRINT "END OF PROGRAM"
110 STOP  : REM Prevent falling into DATA
120 DATA 1,2,3,4,5
130 DATA 6,7,8,9,10
```

**Rule:** Always use STOP before DATA statements if they follow executable code.

---

### 15. GO TO vs GO SUB Confusion

**Problem:** GO TO jumps without return address. GO SUB saves return for RETURN.

**Wrong:**
```basic
10 PRINT "START"
20 GO TO 100  : REM Jump to subroutine
30 PRINT "BACK"  : REM This line never executes

100 REM Subroutine
110 PRINT "SUB"
120 RETURN  : REM ERROR: No GO SUB, so no return address!
```

**Correct:**
```basic
10 PRINT "START"
20 GO SUB 100  : REM Call subroutine
30 PRINT "BACK"  : REM This executes after RETURN
40 STOP

100 REM Subroutine
110 PRINT "SUB"
120 RETURN  : REM Returns to line 30
```

**Rule:** GO SUB for subroutines that RETURN. GO TO for unconditional jumps.

---

## Sound Issues

### 16. BEEP Blocks Execution

**Problem:** BEEP command blocks program execution until sound finishes.

**Symptoms:**
- Game pauses during sound effects
- Animation stops during BEEP
- No concurrent sound and graphics

**Behavior:**
```basic
10 FOR i=1 TO 10
20   PLOT i*20,88
30   BEEP 0.5,0  : REM Program PAUSES for 0.5 seconds!
40 NEXT i
50 REM Each pixel waits for previous beep to finish
```

**Workaround (Short Beeps):**
```basic
10 FOR i=1 TO 10
20   PLOT i*20,88
30   BEEP 0.01,i  : REM Very short beep (10ms)
40 NEXT i
50 REM Minimal interruption
```

**Workaround (Machine Code):**
```basic
10 REM Use machine code for non-blocking sound
20 REM Or accept that BEEP blocks (limitation)
```

**Rule:** BEEP blocks execution. Use short durations or accept the limitation.

---

### 17. BEEP Pitch is in Semitones

**Problem:** BEEP pitch is semitones from middle C, not frequency in Hz.

**Wrong:**
```basic
10 BEEP 0.5,440  : REM Expecting 440 Hz (A note)
20 REM Actually plays 440 semitones above middle C!
```

**Correct:**
```basic
10 BEEP 0.5,0   : REM Middle C (261.6 Hz)
20 BEEP 0.5,9   : REM A above middle C (440 Hz)
30 BEEP 0.5,12  : REM One octave above middle C
40 BEEP 0.5,-12 : REM One octave below middle C
```

**Musical Notes (from middle C = 0):**
```
C:0  D:2  E:4  F:5  G:7  A:9  B:11  C:12
```

**Rule:** BEEP pitch is semitones, not Hz. 0 = middle C, 12 = one octave higher.

---

## Summary of Critical Rules

1. ✅ **Screen bitmap uses thirds formula** - not linear
2. ✅ **Attributes are linear** - not thirds
3. ✅ **Colour clash: 1 INK + 1 PAPER per 8×8 cell**
4. ✅ **PRINT AT: (row, col) from top-left**
5. ✅ **PLOT: (x, y) from bottom-left**
6. ✅ **INKEY$ returns "" when no key** - always check
7. ✅ **Colour values: 0-7 only**
8. ✅ **BRIGHT/FLASH: 0 or 1 only**
9. ✅ **String slicing uses TO** not colons
10. ✅ **BEEP blocks execution** - use short durations
11. ✅ **Arrays auto-DIM to 10** - always DIM explicitly
12. ✅ **BORDER is global** - not per-cell
13. ✅ **STOP before DATA** - prevent fall-through
14. ✅ **GO SUB for returns** - GO TO for jumps
15. ✅ **OVER 1 = XOR mode** - draw twice to erase

---

**This document should be consulted before writing any ZX Spectrum lesson code to avoid these common pitfalls.**
