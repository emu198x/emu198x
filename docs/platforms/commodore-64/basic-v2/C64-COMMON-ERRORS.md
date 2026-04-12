# C64 BASIC V2 Common Errors and Pitfalls

**Purpose:** Document common mistakes in C64 BASIC V2 programming and how to avoid them
**Audience:** C64 curriculum designers and BASIC programmers
**Last Updated:** 2025-11-16

---

## Critical Language Constraints

### 1. Reserved System Variables (NEVER USE)

**Problem:** Certain variable names are reserved by the system and cause errors or unpredictable behaviour.

**Symptoms:**
- `?SYNTAX ERROR` when trying to assign to variable
- Program crashes or hangs
- Timer/I/O operations behave incorrectly

**Reserved Variables:**
```basic
ST  - I/O Status register (READ ONLY - causes ?SYNTAX ERROR when written)
TI  - Jiffy clock (60Hz timer, read/write but conflicts with game timing)
TI$ - Time string (HH:MM:SS format, can cause runtime issues)
```

**Wrong:**
```basic
10 ST=0           : REM ?SYNTAX ERROR - Cannot assign to ST
20 TI=100         : REM Works but conflicts with system timer
30 PRINT TI$      : REM Displays time but can cause timing bugs
```

**Correct:**
```basic
10 GS=0           : REM Game state
20 CT=0           : REM Custom timer
30 SC=0           : REM Score
40 HP=20          : REM Health points
50 LV=1           : REM Level
```

**Validation Check:**
```bash
grep -iE '\<(st|ti|ti\$)\>' example.bas
# Should return NOTHING (no matches = safe)
```

**Rule:** Use `GS`, `CT`, `SC`, `HP`, `LV` or any other 1-2 letter variable names. NEVER use `ST`, `TI`, or `TI$`.

---

### 2. GOSUB Without Target Line

**Problem:** Calling GOSUB to a non-existent line number causes immediate crash.

**Symptoms:**
- `?UNDEF'D STATEMENT ERROR IN [line]`
- Program stops immediately

**Wrong:**
```basic
100 GOSUB 1000    : REM Jump to subroutine
110 PRINT "DONE"
120 END
REM Line 1000 doesn't exist!
```

**Correct:**
```basic
100 GOSUB 1000    : REM Jump to subroutine
110 PRINT "DONE"
120 END

1000 REM --- SUBROUTINE START ---
1010 PRINT "SUBROUTINE"
1020 RETURN
```

**Rule:** ALWAYS ensure the target line exists before using GOSUB. Every GOSUB must have a matching RETURN.

---

## Critical Hardware Quirks

### 3. Screen Memory vs. PETSCII Codes

**Problem:** The code you POKE to screen memory is NOT the same as PETSCII codes used with PRINT.

**Symptoms:**
- Wrong characters appear
- `@` appears instead of `A`
- Graphics characters instead of letters

**Key Difference:**
| Character | PETSCII (PRINT) | Screen Code (POKE) |
|-----------|-----------------|-------------------|
| A         | 65              | 1                 |
| Space     | 32              | 32                |
| @         | 64              | 0                 |

**Wrong:**
```basic
100 POKE 1024,65  : REM Trying to display "A" - shows "@" instead!
```

**Correct:**
```basic
100 POKE 1024,1   : REM Screen code 1 = "A"
110 REM OR use PETSCII with PRINT:
120 PRINT CHR$(65) : REM PETSCII 65 = "A"
```

**Rule:** Use screen codes with POKE, PETSCII codes with PRINT/CHR$. See [PETSCII-AND-SCREEN-CODES.md](../hardware/PETSCII-AND-SCREEN-CODES.md) for conversion tables.

---

### 4. Sprite X-Coordinate Range (0-511 requires MSB)

**Problem:** Sprite X registers only hold 0-255, but screen is 320 pixels wide. Values >255 require setting bit in register $D010.

**Symptoms:**
- Sprite jumps to left side when X > 255
- Sprite wraps around screen edge
- Sprite movement breaks at screen boundary

**Wrong:**
```basic
100 X=300         : REM X position beyond 255
110 POKE 53248,X  : REM X wraps to 44, sprite jumps to left!
```

**Correct:**
```basic
100 X=300
110 POKE 53248,X AND 255              : REM Low byte (0-255)
120 IF X>255 THEN POKE 53264,PEEK(53264)OR 1:GOTO 140
130 POKE 53264,PEEK(53264)AND 254     : REM Clear MSB
140 REM Sprite 0 now correctly at X=300
```

**Boundary-Safe Movement:**
```basic
100 X=X+DX:Y=Y+DY                     : REM Update position
110 IF X<0 THEN X=0                   : REM Clamp to left
120 IF X>319 THEN X=319               : REM Clamp to right
130 IF Y<0 THEN Y=0                   : REM Clamp to top
140 IF Y>199 THEN Y=199               : REM Clamp to bottom
150 POKE 53248,X AND 255              : REM Set low byte
160 IF X>255 THEN POKE 53264,PEEK(53264)OR 1:GOTO 180
170 POKE 53264,PEEK(53264)AND 254     : REM Clear MSB
180 POKE 53249,Y                      : REM Set Y position
```

**Rule:** Always handle X > 255 by setting bit 0 of $D010 (53264). Bit 0 = sprite 0, bit 1 = sprite 1, etc.

---

### 5. Negative or Out-of-Range Values in POKE

**Problem:** POKEing negative values or values > 255 causes `?ILLEGAL QUANTITY ERROR`.

**Symptoms:**
- `?ILLEGAL QUANTITY ERROR IN [line]`
- Program crashes when sprite/character moves off screen

**Wrong:**
```basic
100 Y=Y-1         : REM Y can go negative!
110 POKE 53249,Y  : REM ?ILLEGAL QUANTITY ERROR if Y<0
```

**Correct:**
```basic
100 Y=Y-1
110 IF Y>=0 AND Y<=255 THEN POKE 53249,Y
120 REM Or clamp:
130 IF Y<0 THEN Y=0
140 IF Y>199 THEN Y=199
150 POKE 53249,Y
```

**Rule:** ALWAYS validate values before POKE. Use boundary checks or clamping.

---

## Critical Initialization Bugs

### 6. Camera/Scroll Initialization Bug

**Problem:** Initialising camera and last-drawn positions to same value causes first screen draw to be skipped.

**Symptoms:**
- First screen render doesn't appear
- Game world appears blank until camera moves
- Initial frame is missing

**Wrong:**
```basic
100 CX=0:LX=0     : REM Both zero - first draw skipped!
110 IF CX<>LX THEN GOSUB 2000: REM Condition false on first frame
```

**Correct:**
```basic
100 CX=0:LX=-999  : REM Force first draw by making LX different
110 IF CX<>LX THEN GOSUB 2000: REM Draws on first frame
```

**Rule:** Use impossible value (-999) for last-drawn position to force initial draw.

---

## Performance Pitfalls

### 7. BASIC V2 Speed Limitations

**Problem:** BASIC V2 is interpreted, making it very slow compared to assembly language.

**Impact:**
- Maximum ~50 simple operations per frame (at 60fps)
- Complex calculations cause visible slowdown
- Multiple sprites or enemies become sluggish

**Slow Operations:**
```basic
REM These are SLOW in BASIC V2:
FOR/NEXT loops (especially nested)
Floating-point maths
String operations (LEFT$, MID$, etc.)
Array access (especially multi-dimensional)
```

**Optimisation Strategies:**
```basic
REM 1. Use integers instead of floats when possible
10 X%=100:Y%=50  : REM Integer variables (add % suffix)

REM 2. Pre-calculate constant values
20 SC=1024:CC=55296  : REM Screen and colour memory base

REM 3. Use lookup tables instead of calculation
30 DIM XT(39):FOR I=0 TO 39:XT(I)=SC+I:NEXT  : REM X positions

REM 4. Minimise string operations
40 A$="SCORE:"+STR$(SC)  : REM Build once, not every frame

REM 5. Keep loops short and simple
50 FOR I=0 TO 7:POKE 53269,PEEK(53269)OR(2^I):NEXT  : REM Enable sprites
```

**Rule:** BASIC V2 games require careful optimisation. Consider assembly for time-critical code (see [BASIC-TO-ML-INTEGRATION.md](BASIC-TO-ML-INTEGRATION.md)).

---

### 8. Screen Memory POKEs and Flicker

**Problem:** POKEing directly to screen memory during visible scan can cause flicker.

**Symptoms:**
- Screen flickers when updating
- Partial characters visible mid-update
- "Tearing" effect

**Wrong:**
```basic
100 FOR I=0 TO 999:POKE 1024+I,32:NEXT  : REM Clear screen - visible redraw!
```

**Correct (Simple):**
```basic
100 PRINT CHR$(147)  : REM Use built-in clear screen (fast)
```

**Correct (Advanced):**
```basic
100 REM Wait for raster line 250 (bottom of screen)
110 IF PEEK(53266)<250 THEN 110
120 FOR I=0 TO 999:POKE 1024+I,32:NEXT  : REM Update during VBlank
```

**Rule:** Use built-in screen commands when possible. For direct POKEs, wait for vertical blank.

---

## Memory Constraints

### 9. BASIC Program Size Limits

**Problem:** BASIC programs share memory with variables and arrays.

**Limits:**
- ~38KB available for program + variables (on stock C64)
- Larger programs leave less room for arrays/strings
- `?OUT OF MEMORY ERROR` when limit exceeded

**Symptoms:**
- `?OUT OF MEMORY ERROR`
- Program won't load
- Arrays won't dimension

**Mitigation:**
```basic
REM 1. Use machine language for large routines
REM 2. Break program into parts, load as needed
REM 3. Use external data files
REM 4. Careful variable management (reuse where possible)
```

**Memory Map:**
```
$0801-$9FFF: BASIC program and variables (~38KB)
$A000-$BFFF: BASIC ROM (8KB)
$C000-$CFFF: Usually free RAM (4KB - can use for ML routines)
```

**Rule:** Keep BASIC programs concise. Use machine language for complex routines.

---

## References

- **PETSCII vs Screen Codes:** [PETSCII-AND-SCREEN-CODES.md](../hardware/PETSCII-AND-SCREEN-CODES.md)
- **BASIC V2 Reference:** [BASIC-V2-REFERENCE.md](BASIC-V2-REFERENCE.md)
- **Error Messages:** [BASIC-V2-ERROR-MESSAGES-REFERENCE.md](BASIC-V2-ERROR-MESSAGES-REFERENCE.md)
- **VIC-II (Sprites):** [../hardware/VIC-II-QUICK-REFERENCE.md](../hardware/VIC-II-QUICK-REFERENCE.md)

---

**Document Version:** 1.0
**Last Updated:** 2025-11-16
**Based on:** C64 BASIC V2 specification and common lesson creation pitfalls
