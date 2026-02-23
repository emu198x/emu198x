# Bitmap Graphics Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

Bitmap mode provides direct pixel-level control of the screen, enabling high-resolution graphics, detailed pictures, charts, and visual effects. Each individual dot (pixel) on the screen is controlled by a bit in memory.

**Resolution:**
- **Standard bitmap:** 320×200 pixels (64,000 dots), 2 colors per 8×8 cell
- **Multicolor bitmap:** 160×200 pixels (32,000 dots), 4 colors per 8×8 cell

**Memory requirement:** 8000 bytes (8K) for bitmap data

---

## Standard High-Resolution Bitmap Mode

### Specifications

- **Resolution:** 320 horizontal × 200 vertical pixels
- **Memory required:** 8000 bytes for bitmap data
- **Color control:** 2 colors per 8×8 cell (from screen memory)
- **Typical use:** Detailed graphics, charts, precise drawing

### Enabling Bitmap Mode

**Register:** 53265 ($D011), bit 5

```basic
REM Turn bitmap mode ON
POKE 53265, PEEK(53265) OR 32

REM Turn bitmap mode OFF
POKE 53265, PEEK(53265) AND 223
```

### How Bitmap Mode Works

**Relationship to character mode:**
- Screen is filled with "programmable characters"
- Bitmap memory = character pattern data
- Screen memory (1024-2023) = color information (NOT character codes)

**Color control via screen memory:**
- **Upper 4 bits:** Color for pixels set to 1
- **Lower 4 bits:** Color for pixels set to 0

**Example:**
```basic
REM Screen memory value 3 ($03 hex = 0000 0011 binary)
REM Upper 4 bits = 0 (black) for pixels=1
REM Lower 4 bits = 3 (cyan) for pixels=0
POKE 1024, 3  : REM Top-left cell: black dots on cyan background
```

---

## Bitmap Memory Location

### Positioning the Bitmap

**Control Register:** 53272 ($D018), bit 3

```basic
REM Set bitmap location
POKE 53272, PEEK(53272) OR 8   : REM Bitmap at 8192 ($2000)
POKE 53272, PEEK(53272) AND 247 : REM Bitmap at 0 ($0000)
```

**Bit 3 value:**
- **0:** Bitmap starts at offset 0 within VIC-II bank
- **1:** Bitmap starts at offset 8192 ($2000) within VIC-II bank

**Common locations:**
- **8192** ($2000) in Bank 0 - Most common for BASIC
- **0** ($0000) in Bank 1 or 3 - For maximum flexibility

---

## Bitmap Memory Organization

### Memory Layout

Bitmap is organized in 25 rows of 40 character cells:
- Each cell = 8×8 pixels = 64 bits = 8 bytes
- Total = 1000 cells × 8 bytes = 8000 bytes

**Layout pattern:**
```
Row 0, Char 0:  Bytes 0-7
Row 0, Char 1:  Bytes 8-15
Row 0, Char 2:  Bytes 16-23
...
Row 0, Char 39: Bytes 312-319

Row 1, Char 0:  Bytes 320-327
Row 1, Char 1:  Bytes 328-335
...
```

**Formula for character cell:**
```
Cell Address = BASE + (ROW × 320) + (CHAR × 8) + LINE
```

Where:
- BASE = Bitmap start address (0 or 8192)
- ROW = Screen row (0-24)
- CHAR = Character column (0-39)
- LINE = Line within character (0-7)

---

## Pixel Addressing Formulas

### Coordinate System

```
   X →
Y  0─────────────────────────────────────319
↓  │
   │         Screen coordinates
   │         X: 0-319 (horizontal)
   │         Y: 0-199 (vertical)
   │
  199────────────────────────────────────────
```

### Calculating Byte and Bit

For pixel at coordinate (X, Y):

```basic
REM Step 1: Determine character cell
ROW = INT(Y / 8)          : REM 0-24
CHAR = INT(X / 8)         : REM 0-39

REM Step 2: Determine line within character
LINE = Y AND 7            : REM 0-7

REM Step 3: Calculate byte address
BYTE = BASE + ROW * 320 + CHAR * 8 + LINE

REM Step 4: Determine bit within byte
BIT = 7 - (X AND 7)       : REM 7-0 (leftmost to rightmost)
```

### Setting a Pixel

```basic
REM Turn pixel ON (set bit to 1)
POKE BYTE, PEEK(BYTE) OR (2 ^ BIT)

REM Turn pixel OFF (set bit to 0)
POKE BYTE, PEEK(BYTE) AND (255 - (2 ^ BIT))

REM Toggle pixel
POKE BYTE, PEEK(BYTE) XOR (2 ^ BIT)
```

---

## Complete Bitmap Setup

### Initialization Sequence

```basic
10 REM Bitmap at 8192
20 BASE = 2 * 4096
30 POKE 53272, PEEK(53272) OR 8
40 REM Enable bitmap mode
50 POKE 53265, PEEK(53265) OR 32
60 REM Clear bitmap memory
70 FOR I = BASE TO BASE + 7999
80 POKE I, 0
90 NEXT I
100 REM Set colors (cyan on black)
110 FOR I = 1024 TO 2023
120 POKE I, 3  : REM Upper 4 bits=0 (black), lower 4 bits=3 (cyan)
130 NEXT I
```

### Memory Protection for BASIC

**Problem:** BASIC variables can overwrite bitmap

**Solution:** Move BASIC's top-of-memory pointer

```basic
REM Protect memory from 8192 upward
POKE 52, 32 : POKE 56, 32 : CLR
```

**Result:**
- BASIC program: 2048-8191
- Bitmap data: 8192-16191 (protected)

---

## Multicolor Bitmap Mode

### Specifications

- **Resolution:** 160 horizontal × 200 vertical pixels (pixels are 2× wider)
- **Memory required:** 8000 bytes for bitmap data
- **Colors:** 4 colors per 8×8 cell
- **Trade-off:** Half horizontal resolution for 4× color options

### Enabling Multicolor Bitmap Mode

```basic
REM Turn ON multicolor bitmap mode
POKE 53265, PEEK(53265) OR 32   : REM Bitmap mode
POKE 53270, PEEK(53270) OR 16   : REM Multicolor mode

REM Turn OFF multicolor bitmap mode
POKE 53265, PEEK(53265) AND 223 : REM Bitmap mode off
POKE 53270, PEEK(53270) AND 239 : REM Multicolor mode off
```

### Color Sources

In multicolor bitmap mode, bit pairs determine colors:

| Bit Pair | Color Source | Register/Location |
|----------|-------------|-------------------|
| 00 | Background color #0 | 53281 ($D021) |
| 01 | Upper 4 bits of screen memory | Screen memory (1024-2023) |
| 10 | Lower 4 bits of screen memory | Screen memory (1024-2023) |
| 11 | Color memory (nybble) | Color RAM (55296-56295) |

**Example:**
```basic
REM Set up 4 colors for a cell
POKE 53281, 0        : REM Background = black (bit pair 00)
POKE 1024, 16 * 2 + 7 : REM Upper 4 bits = red (01), lower 4 bits = yellow (10)
POKE 55296, 1        : REM Color RAM = white (bit pair 11)
```

### Multicolor Pixel Addressing

**Coordinate system:** (X, Y) where X = 0-159, Y = 0-199

```basic
REM Calculate byte and bit-pair
ROW = INT(Y / 8)
CHAR = INT(X / 4)         : REM 4 double-wide pixels per character
LINE = Y AND 7
BYTE = BASE + ROW * 320 + CHAR * 8 + LINE

REM Bit pair position (0-3, left to right)
BITPAIR = 3 - (X AND 3)

REM Calculate bit pair value (0-3)
REM Then POKE appropriate pattern
```

---

## Practical Examples

### Example 1: Plot a Sine Wave

```basic
5 BASE=8192:POKE53272,PEEK(53272)OR8
10 POKE53265,PEEK(53265)OR32
20 FORI=BASETOBASE+7999:POKEI,0:NEXT
30 FORI=1024TO2023:POKEI,3:NEXT
50 FORX=0TO319STEP.5
60 Y=INT(90+80*SIN(X/10))
70 CH=INT(X/8)
80 RO=INT(Y/8)
85 LN=YAND7
90 BY=BASE+RO*320+8*CH+LN
100 BI=7-(XAND7)
110 POKEBY,PEEK(BY)OR(2^BI)
120 NEXTX
130 GOTO130
```

### Example 2: Draw a Circle

```basic
5 BASE=8192:POKE53272,PEEK(53272)OR8
10 POKE53265,PEEK(53265)OR32
20 FORI=BASETOBASE+7999:POKEI,0:NEXT
30 FORI=1024TO2023:POKEI,16:NEXT
50 FORX=0TO160
55 Y1=100+SQR(160*X-X*X)
56 Y2=100-SQR(160*X-X*X)
60 FORY=Y1TOY2STEPY1-Y2
70 CH=INT(X/8)
80 RO=INT(Y/8)
85 LN=YAND7
90 BY=BASE+RO*320+8*CH+LN
100 BI=7-(XAND7)
110 POKEBY,PEEK(BY)OR(2^BI)
114 NEXTY
120 NEXTX
130 GOTO130
```

### Example 3: Clear Bitmap Screen

```basic
REM Fast clear routine
FOR I = BASE TO BASE + 7999
POKE I, 0
NEXT I
```

### Example 4: Set Screen Colors

```basic
REM Set all cells to white on blue
FOR I = 1024 TO 2023
POKE I, 16 * 1 + 6  : REM Upper 4 bits=1 (white), lower=6 (blue)
NEXT I
```

---

## Important Notes

### BASIC Performance Warning

**Problem:** Bitmap operations in BASIC are VERY slow

**Solutions:**
1. Use machine language routines (SYS calls from BASIC)
2. Use commercial extensions (e.g., VSP cartridge)
3. Pre-calculate and use lookup tables
4. Minimize pixel operations per frame

### Memory Overlap Warning

**Critical:** BASIC variables can overwrite bitmap memory

**Always protect memory:**
```basic
REM MUST be first line in program
POKE 52, 32 : POKE 56, 32 : CLR
```

**Never skip this step** or bitmap will be corrupted by variable storage.

### Character ROM Conflict

Bitmap memory **cannot** overlap character ROM images:
- Bank 0: ROM at $1000-$1FFF (use bitmap at $2000)
- Bank 2: ROM at $9000-$9FFF (use bitmap at $A000)
- Banks 1 & 3: No ROM images (bitmap can be at $0000 or $2000)

---

## Screen Memory vs Color Memory

### In Bitmap Mode

**Screen memory (1024-2023):**
- **Standard bitmap:** Upper 4 bits = color for 1-bits, lower 4 bits = color for 0-bits
- **Multicolor bitmap:** Upper 4 bits = color for 01, lower 4 bits = color for 10

**Color memory (55296-56295):**
- **Standard bitmap:** NOT used
- **Multicolor bitmap:** Color for bit pair 11

### Setting Screen Memory Colors

```basic
REM Standard bitmap: White (1) on blue (6)
POKE 1024, 16 * 1 + 6  : REM $16 = 0001 0110

REM Multicolor bitmap: Red (2), yellow (7), white (1)
POKE 1024, 16 * 2 + 7  : REM $27 = 0010 0111
POKE 55296, 1          : REM White for bit pair 11
```

---

## Coordinate Reference

### Visible Screen Boundaries

| Boundary | Standard (320×200) | Multicolor (160×200) |
|----------|-------------------|---------------------|
| Top-left | (0, 0) | (0, 0) |
| Top-right | (319, 0) | (159, 0) |
| Bottom-left | (0, 199) | (0, 199) |
| Bottom-right | (319, 199) | (159, 199) |
| Center | (160, 100) | (80, 100) |

### Cell Grid

- **40 columns** × **25 rows** = 1000 cells
- Each cell = 8×8 pixels (standard) or 4×8 pixels (multicolor)

**Cell position for pixel (X, Y):**
```
Column = INT(X / 8)   : REM 0-39
Row = INT(Y / 8)      : REM 0-24
```

---

## Conversion Formulas

### Pixel to Screen Memory

For pixel (X, Y), get corresponding screen memory location:

```basic
REM Calculate which cell controls this pixel's colors
COL = INT(X / 8)
ROW = INT(Y / 8)
SCREEN_ADDR = 1024 + (ROW * 40) + COL
```

### Byte Offset to Coordinates

Given byte offset from BASE:

```basic
OFFSET = BYTE - BASE
LINE = OFFSET AND 7
CELL = INT(OFFSET / 8)
CHAR = CELL MOD 40
ROW = INT(CELL / 40)
```

---

## Quick Reference

### Key Registers

| Register | Function | Bit | Value |
|----------|----------|-----|-------|
| 53265 | Bitmap mode enable | 5 | 1=ON, 0=OFF |
| 53270 | Multicolor mode | 4 | 1=ON, 0=OFF |
| 53272 | Bitmap location | 3 | 1=$2000, 0=$0000 |
| 53281 | Background color #0 | 0-3 | 0-15 |

### Common Bitmap Addresses

| Location | Decimal | Hex | Notes |
|----------|---------|-----|-------|
| Bank 0 bitmap | 8192 | $2000 | Most common |
| Bank 1 bitmap | 16384 | $4000 | No ROM conflict |
| Bank 1 bitmap alt | 24576 | $6000 | Alternative |

### Memory Protection

| BASIC Top | Decimal | Protects From | Use For |
|-----------|---------|---------------|---------|
| POKE 52,32 | 8192 | 8192+ | Bitmap at $2000 |
| POKE 52,48 | 12288 | 12288+ | Chars at $3000 |
| POKE 52,64 | 16384 | 16384+ | Bitmap at $4000 |

---

## Common Pitfalls

### Forgetting to Clear Bitmap

**Symptom:** Random garbage on screen
**Solution:** Always clear bitmap memory after enabling mode

### Wrong Color in Screen Memory

**Symptom:** Unexpected colors
**Solution:** Remember upper 4 bits = 1-bits color, lower 4 bits = 0-bits color

### Memory Not Protected

**Symptom:** Bitmap gets corrupted during program execution
**Solution:** Use POKE 52/56 at program start

### Slow Performance

**Symptom:** Drawing takes forever
**Solution:** Use machine language, minimize pixel operations, use lookup tables

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for mode overview
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for memory maps

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
