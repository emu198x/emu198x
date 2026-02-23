# Programmable Characters Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

The C64 allows you to create custom 8×8 character patterns in RAM, replacing or supplementing the built-in ROM character set. This enables:
- Custom graphics for games
- Special symbols for applications
- Animated characters
- Full control over visual appearance

---

## Character Set Basics

### Character Set Structure

- **Full character set:** 256 characters × 8 bytes = **2048 bytes (2K)**
- **Each character:** 8 bytes (one byte per row)
- **Each byte:** 8 bits (one bit per pixel)
- **Bit value:** 1 = foreground color, 0 = background color

### Memory Requirements

A character set **must** start at a 2K boundary within the VIC-II's 16K bank:

| Offset | Decimal | Hex |
|--------|---------|-----|
| 0 | 0 | $0000 |
| 2K | 2048 | $0800 |
| 4K | 4096 | $1000 |
| 6K | 6144 | $1800 |
| 8K | 8192 | $2000 |
| 10K | 10240 | $2800 |
| 12K | 12288 | $3000 |
| 14K | 14336 | $3800 |

---

## BASIC Memory Protection

### Why Protection Is Needed

BASIC programs also use RAM. Without protection, your BASIC program could overwrite your character set, or vice versa.

### How to Protect Memory

```basic
REM Move BASIC's top-of-memory pointer
POKE 52, 48 : POKE 56, 48 : CLR
```

**What this does:**
- POKE 52, 48: Sets high byte of BASIC memory end to $30 (48)
- POKE 56, 48: Sets high byte of string storage end to $30 (48)
- CLR: Resets BASIC variables with new memory layout

**Result:** Memory from 12288 ($3000) upward is protected from BASIC

### Memory Map After Protection

```
0-2047      System (zero page, stack, etc.)
2048-12287  BASIC program space (10K)
12288-14335 Character set (2K) - PROTECTED
14336-40959 Free RAM (26K+)
```

---

## Recommended Character Set Location

**For BASIC programs: 12288 ($3000)**

**Advantages:**
- Valid 2K boundary
- Easy to remember ($3000 hex)
- Leaves 10K for BASIC programs
- Simple activation code (POKE value = 12)

**Alternative:** 14336 ($3800) - if you need more BASIC program space

---

## Copying Characters from ROM

### Complete Copy Routine

```basic
10 REM Copy 64 characters from ROM to RAM
20 POKE 56334, PEEK(56334) AND 254 : REM Turn off interrupts
30 POKE 1, PEEK(1) AND 251 : REM Switch in character ROM
40 FOR I = 0 TO 511
50 POKE 12288 + I, PEEK(53248 + I)
60 NEXT I
70 POKE 1, PEEK(1) OR 4 : REM Switch in I/O
80 POKE 56334, PEEK(56334) OR 1 : REM Turn on interrupts
```

### Why the Switch Sequence?

**Problem:** Character ROM (53248-57343) shares address space with I/O registers

**Solution:**
1. Turn off interrupts (they need I/O to work)
2. Switch ROM into address space
3. Copy data
4. Switch I/O back
5. Turn interrupts back on

**Critical:** NEVER skip steps 2 or 7. System will crash if interrupts fire while I/O is switched out.

### Copying Full Character Set (256 chars)

Change line 40 to:
```basic
40 FOR I = 0 TO 2047
```

### Copying Specific Characters

```basic
REM Copy character 65 (letter A)
REM Character starts at ROM address: 53248 + (65 × 8) = 53768
REM Destination in RAM: 12288 + (65 × 8) = 12808

FOR I = 0 TO 7
POKE 12808 + I, PEEK(53768 + I)
NEXT I
```

---

## Activating Custom Character Set

### Point VIC-II to Your Character Set

```basic
POKE 53272, (PEEK(53272) AND 240) + 12
```

**What this does:**
- Reads current value of register 53272
- Clears lower 4 bits (character memory pointer)
- Sets value to 12 (points to 12288 / $3000)
- Preserves upper 4 bits (screen memory pointer)

### Deactivating (Back to ROM)

```basic
POKE 53272, (PEEK(53272) AND 240) + 4
```

Value 4 points back to character ROM image at $1000.

---

## Creating Character Patterns

### Binary to Decimal Conversion

Each row of a character is one byte. Calculate the decimal value by adding bit position values:

```
Bit:     7    6   5   4   3   2   1   0
Value: 128   64  32  16   8   4   2   1
```

**Example: Smiley face row**
```
Row pattern: **  **
Binary:      01100110
Calculation: 64 + 32 + 4 + 2 = 102
```

### Complete Smiley Face Character

```basic
10 REM Define smiley face at character position 20
20 BASE = 12288 + (20 × 8)
30 FOR I = 0 TO 7
40 READ A
50 POKE BASE + I, A
60 NEXT I
70 DATA 60, 66, 165, 129, 165, 153, 66, 60
```

**Visual breakdown:**

| Row | Binary | Decimal | Visual |
|-----|--------|---------|--------|
| 0 | 00111100 | 60 | `  ****  ` |
| 1 | 01000010 | 66 | ` *    * ` |
| 2 | 10100101 | 165 | `* *  * *` |
| 3 | 10000001 | 129 | `*      *` |
| 4 | 10100101 | 165 | `* *  * *` |
| 5 | 10011001 | 153 | `*  **  *` |
| 6 | 01000010 | 66 | ` *    * ` |
| 7 | 00111100 | 60 | `  ****  ` |

---

## Character Design Worksheet

Use this grid to design characters. Fill in squares where you want pixels, then calculate row values.

```
Bit:    7   6   5   4   3   2   1   0   Total
Value: 128  64  32  16   8   4   2   1

Row 0  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 1  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 2  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 3  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 4  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 5  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 6  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
Row 7  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]  = ___
```

**How to calculate row values:**
1. Mark which squares you want filled (with X or checkmark)
2. For each filled square, write down the bit value from the header
3. Add up all the bit values for that row
4. That's the DATA value for that row

---

## Character Address Calculation

### Formula

```
Character Address = Base Address + (Character Code × 8)
```

### Examples

For character set at 12288 ($3000):

| Character | Code | Calculation | Address |
|-----------|------|-------------|---------|
| @ | 0 | 12288 + (0 × 8) | 12288 |
| A | 1 | 12288 + (1 × 8) | 12296 |
| B | 2 | 12288 + (2 × 8) | 12304 |
| SPACE | 32 | 12288 + (32 × 8) | 12544 |
| 0 (zero) | 48 | 12288 + (48 × 8) | 12672 |

### Range for Each Character

Each character occupies 8 consecutive bytes:

**Character "A" (code 1):**
- Starts at: 12296
- Ends at: 12303 (12296 + 7)
- Row 0: 12296
- Row 1: 12297
- Row 2: 12298
- Row 3: 12299
- Row 4: 12300
- Row 5: 12301
- Row 6: 12302
- Row 7: 12303

---

## Modifying Existing Characters

### Reverse a Character

```basic
REM Reverse character at position 20
FOR I = 0 TO 7
ADDR = 12288 + (20 × 8) + I
POKE ADDR, 255 - PEEK(ADDR)
NEXT I
```

### Make Solid Block

```basic
REM Create solid block at character position 60
FOR I = 0 TO 7
POKE 12288 + (60 × 8) + I, 255
NEXT I
```

### Clear Character (Make Blank)

```basic
REM Clear character at position 30
FOR I = 0 TO 7
POKE 12288 + (30 × 8) + I, 0
NEXT I
```

---

## Common Patterns and Values

### Useful Character Patterns

| Description | Binary | Decimal |
|-------------|--------|---------|
| Blank row | 00000000 | 0 |
| Full row | 11111111 | 255 |
| Left half | 11110000 | 240 |
| Right half | 00001111 | 15 |
| Checkerboard 1 | 10101010 | 170 |
| Checkerboard 2 | 01010101 | 85 |
| Center dot | 00011000 | 24 |
| Vertical line (centre) | 00011000 | 24 |
| Horizontal line | Any value | (varies) |

### Character Building Blocks

**Horizontal bars:**
```
Top:    11111111 (255)
Upper:  11111111 (255)
Middle: 11111111 (255)
Lower:  11111111 (255)
Bottom: 11111111 (255)
```

**Vertical bars:**
```
Left edge:   10000000 (128)
Left middle: 11000000 (192)
Centre:      00011000 (24)
Right middle:00000011 (3)
Right edge:  00000001 (1)
```

---

## Complete Example Programs

### Example 1: Basic Character Creator

```basic
10 REM Protect memory and copy ROM chars
20 POKE 52,48:POKE 56,48:CLR
30 POKE 56334,PEEK(56334)AND 254
40 POKE 1,PEEK(1)AND 251
50 FOR I=0 TO 511
60 POKE 12288+I,PEEK(53248+I)
70 NEXT I
80 POKE 1,PEEK(1)OR 4
90 POKE 56334,PEEK(56334)OR 1
100 REM Activate custom character set
110 POKE 53272,(PEEK(53272)AND 240)+12
120 REM Create custom character at position 20
130 FOR I=0 TO 7
140 READ A
150 POKE 12288+(20*8)+I,A
160 NEXT I
170 DATA 60,66,165,129,165,153,66,60
180 REM Display the character
190 PRINT CHR$(147)
200 PRINT CHR$(20);
210 END
```

### Example 2: Animated Character

```basic
10 REM Set up character set
20 POKE 52,48:POKE 56,48:CLR
30 GOSUB 1000: REM Copy ROM chars
40 POKE 53272,(PEEK(53272)AND 240)+12
50 REM Animation loop
60 PRINT CHR$(147)CHR$(20);
70 FOR F=1 TO 4
80 RESTORE
90 FOR I=0 TO 7
100 READ A
110 POKE 12288+(20*8)+I,A
120 NEXT I
130 FOR D=1 TO 50:NEXT D
140 NEXT F
150 GOTO 70
160 DATA 60,66,165,129,165,153,66,60
170 DATA 60,66,165,129,129,189,66,60
180 DATA 60,66,129,165,129,189,66,60
190 DATA 60,66,129,129,165,189,66,60
1000 REM Copy ROM characters subroutine
1010 POKE 56334,PEEK(56334)AND 254
1020 POKE 1,PEEK(1)AND 251
1030 FOR I=0 TO 511
1040 POKE 12288+I,PEEK(53248+I)
1050 NEXT I
1060 POKE 1,PEEK(1)OR 4
1070 POKE 56334,PEEK(56334)OR 1
1080 RETURN
```

---

## Important Technical Details

### Character ROM Availability

**Character ROM is ONLY available in VIC-II Banks 0 and 2:**
- Bank 0 ($0000-$3FFF): ROM appears at $1000-$1FFF
- Bank 2 ($8000-$BFFF): ROM appears at $9000-$9FFF
- Banks 1 and 3: No ROM image available

### Preventing Chroma Noise

**Always make vertical lines at least 2 bits wide** in character designs. Single-pixel vertical lines can cause color distortion on TV screens due to NTSC/PAL chroma encoding.

**Bad (1 bit wide):**
```
00010000 (16)
```

**Good (2 bits wide):**
```
00011000 (24)
```

### All-or-Nothing Nature

When you point the VIC-II to custom character RAM:
- **ALL** characters come from RAM
- ROM characters are no longer accessible
- You must copy any ROM characters you want to keep

**Strategy:** Copy ROM characters first, then modify or replace only the ones you need.

---

## Troubleshooting

### Characters Look Like Garbage

**Cause:** Character memory not initialized
**Solution:** Copy ROM characters or initialize all bytes to valid patterns

### System Crashes When Copying

**Cause:** Forgot to disable interrupts
**Solution:** Always use `POKE 56334,PEEK(56334)AND 254` before switching ROM in

### Characters Don't Change

**Cause:** VIC-II still pointing to ROM character set
**Solution:** Use `POKE 53272,(PEEK(53272)AND 240)+12`

### BASIC Program Overwrites Characters

**Cause:** Memory not protected
**Solution:** Use `POKE 52,48:POKE 56,48:CLR` at start of program

---

## Quick Reference

### Complete Setup Sequence

```basic
REM 1. Protect memory
POKE 52,48:POKE 56,48:CLR

REM 2. Copy ROM characters
POKE 56334,PEEK(56334)AND 254
POKE 1,PEEK(1)AND 251
FOR I=0 TO 511:POKE 12288+I,PEEK(53248+I):NEXT I
POKE 1,PEEK(1)OR 4
POKE 56334,PEEK(56334)OR 1

REM 3. Activate character set
POKE 53272,(PEEK(53272)AND 240)+12

REM 4. Modify characters as needed
```

### Key Memory Locations

| Purpose | Address | Formula |
|---------|---------|---------|
| Character set base | 12288 | $3000 |
| Character N address | 12288 + (N × 8) | Base + (code × 8) |
| VIC-II control | 53272 | $D018 |
| Character ROM | 53248 | $D000 (when switched) |

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for mode details
- **Related:** See SCREEN-CODES-REFERENCE.md for character code tables

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
