# Multicolor Character Mode Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

Multicolor mode trades horizontal resolution for additional colors. Instead of 2 colors per character (foreground/background), you get **4 colors per character**, with pixels twice as wide.

**Resolution comparison:**
- Standard mode: 320×200 effective resolution, 2 colors per 8×8 cell
- Multicolor mode: 160×200 effective resolution, 4 colors per 8×8 cell

---

## How Multicolor Mode Works

### The Trade-Off

**Standard Hi-Res Mode:**
- Each bit = 1 pixel
- 8 bits per row = 8 pixels across
- Bit value: 0 = background, 1 = foreground
- Result: 2 colors, 8 pixels wide

**Multicolor Mode:**
- Each bit-pair = 1 pixel (doubled width)
- 8 bits per row = 4 pixels across
- Bit-pair value: 00, 01, 10, or 11 = 4 color choices
- Result: 4 colors, 4 pixels wide (each pixel 2 bits)

### Visual Comparison

**Standard Mode (8 pixels):**
```
Bits:    0 1 1 0 0 1 1 0
Pixels:  □ ■ ■ □ □ ■ ■ □
Width:   1 1 1 1 1 1 1 1
```

**Multicolor Mode (4 double-wide pixels):**
```
Bits:    00  01  10  11
Pixels:  □□  ▓▓  ▒▒  ■■
Width:   2   2   2   2
```

---

## Enabling Multicolor Mode

### Global Multicolor Bit

**Register:** 53270 ($D016), bit 4

```basic
REM Enable multicolor mode capability
POKE 53270, PEEK(53270) OR 16

REM Disable multicolor mode
POKE 53270, PEEK(53270) AND 239
```

**Important:** This only enables the *capability*. Individual characters must still be set to multicolor mode via color memory.

### Per-Character Activation

**Color Memory:** 55296-56295 ($D800-$DBE7), bit 3

Each character position can independently be:
- **Hi-res mode:** Color value 0-7 (bit 3 = 0)
- **Multicolor mode:** Color value 8-15 (bit 3 = 1)

```basic
REM Set screen position 1024 to multicolor mode
POKE 55296, 8   : REM Any value 8-15
```

**The number you POKE determines TWO things:**
1. **Bit 3 (value 8+):** Multicolor mode ON
2. **Bits 0-2 (value 0-7):** Character foreground color

**Example color values:**
- 8 = Multicolor mode, color 0 (black) as foreground
- 9 = Multicolor mode, color 1 (white) as foreground
- 15 = Multicolor mode, color 7 (yellow) as foreground

---

## Color Sources in Multicolor Mode

### The Four Colors

| Bit Pair | Color Name | Source | Register | Location |
|----------|-----------|---------|----------|----------|
| 00 | Background #0 | Shared (screen color) | - | 53281 ($D021) |
| 01 | Background #1 | Shared | Global | 53282 ($D022) |
| 10 | Background #2 | Shared | Global | 53283 ($D023) |
| 11 | Foreground | Individual | Per-char | Color RAM (bits 0-2) |

### Color Limitations

**Background colors (00, 01, 10):**
- Can use **all 16 colors** (0-15)
- Shared across entire screen
- Set via VIC-II registers

**Foreground color (11):**
- Can only use **8 colors** (0-7)
- Individual per character
- Set via color memory (bits 0-2)

**Why the limitation?** Only 3 bits available for character color (bits 0-2), while bit 3 is used for multicolor mode flag.

### Setting Colors

```basic
REM Set up color palette
POKE 53281, 6   : REM Background #0 = Blue (screen color)
POKE 53282, 14  : REM Background #1 = Light blue
POKE 53283, 3   : REM Background #2 = Cyan

REM Set character at position 1024 to multicolor, white foreground
POKE 1024, 65   : REM Character code (letter A)
POKE 55296, 9   : REM Multicolor ON (8) + White (1) = 9
```

---

## Bit Pair Patterns

### Understanding Bit Pairs

Each row of a multicolor character is still 8 bits, but they're interpreted as 4 pairs:

**Example row: 01101011**

| Position | Bits | Bit Pair | Color | Pixel Width |
|----------|------|----------|-------|-------------|
| 0-1 | 01 | 01 | Background #1 | 2 wide |
| 2-3 | 10 | 10 | Background #2 | 2 wide |
| 4-5 | 10 | 10 | Background #2 | 2 wide |
| 6-7 | 11 | 11 | Foreground | 2 wide |

**Visual result:** `[01][10][10][11]` = Four double-wide pixels

### Common Bit Pair Values

| Bit Pair | Binary | Decimal Contribution | Color |
|----------|--------|---------------------|-------|
| 00 | 00 | 0 | Background #0 |
| 01 | 01 | 64 or 16 or 4 or 1 | Background #1 |
| 10 | 10 | 128 or 32 or 8 or 2 | Background #2 |
| 11 | 11 | 192 or 48 or 12 or 3 | Foreground |

### Calculating Row Values

**Example: Create pattern [00][01][10][11]**

```
Bit pairs:   00    01    10    11
Binary:    00011011
Positions: 76543210

Calculation:
  Bit 7 (128): 0
  Bit 6 (64):  0
  Bit 5 (32):  0
  Bit 4 (16):  1 = 16
  Bit 3 (8):   1 = 8
  Bit 2 (4):   0
  Bit 1 (2):   1 = 2
  Bit 0 (1):   1 = 1
Total: 16 + 8 + 2 + 1 = 27
```

---

## Multicolor Character Design

### Design Worksheet

Use this grid for multicolor characters. Each cell is a double-wide pixel.

```
Pixel:    0        1        2        3       Row Value
        [  ][  ] [  ][  ] [  ][  ] [  ][  ]
Bits:     7  6     5  4     3  2     1  0

Row 0:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 1:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 2:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 3:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 4:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 5:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 6:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___
Row 7:  [  ][  ] [  ][  ] [  ][  ] [  ][  ]  = ___

Color codes:
00 = Background #0 (screen color)
01 = Background #1
10 = Background #2
11 = Foreground (character color)
```

### Quick Reference: Bit Pair Contributions

| Pixel Position | Bit Pair | Value if 01 | Value if 10 | Value if 11 |
|----------------|----------|-------------|-------------|-------------|
| 0 (left) | Bits 7-6 | 64 | 128 | 192 |
| 1 | Bits 5-4 | 16 | 32 | 48 |
| 2 | Bits 3-2 | 4 | 8 | 12 |
| 3 (right) | Bits 1-0 | 1 | 2 | 3 |

**To calculate row value:** Add up all contributions for non-00 pixels.

---

## Practical Examples

### Example 1: Simple Multicolor Character

**Goal:** Create a character with 4 distinct colors

```basic
10 REM Set up multicolor mode
20 POKE 53270, PEEK(53270) OR 16
30 POKE 53281, 0  : REM Background #0 = Black
40 POKE 53282, 2  : REM Background #1 = Red
50 POKE 53283, 7  : REM Background #2 = Yellow
60 REM Character data (each row uses all 4 colors)
70 FOR I = 0 TO 7
80 READ A
90 POKE 12288 + I, A
100 NEXT I
110 DATA 27, 27, 27, 27, 27, 27, 27, 27
120 REM Display at position 1024, foreground = white
130 POKE 1024, 0  : REM Character code 0
140 POKE 55296, 9 : REM Multicolor mode (8) + White (1)
```

**Breakdown of value 27:**
- Binary: 00011011
- Bit pairs: 00 | 01 | 10 | 11
- Colors: Black | Red | Yellow | White
- Visual: Four equal vertical stripes

### Example 2: Multicolor Sprite-Like Character

```basic
10 REM Multicolor setup
20 POKE 53270, PEEK(53270) OR 16
30 POKE 53281, 6  : REM Background #0 = Blue
40 POKE 53282, 14 : REM Background #1 = Light blue
50 POKE 53283, 0  : REM Background #2 = Black
60 REM Create character at position 20
70 FOR I = 0 TO 7
80 READ A
90 POKE 12288 + (20 * 8) + I, A
100 NEXT I
110 REM Pattern: Eyes and mouth in different colors
120 DATA 129, 37, 21, 29, 93, 85, 85, 85
130 POKE 1024, 20
140 POKE 55296, 15 : REM Multicolor + Yellow foreground
```

### Example 3: Color Cycling

```basic
10 REM Setup
20 POKE 53270, PEEK(53270) OR 16
30 PRINT CHR$(147)
40 FOR I = 1 TO 100
50 PRINT CHR$(65);
60 POKE 54296 + I, 8 : REM Set to multicolor
70 NEXT I
80 REM Cycle colors
90 FOR C = 0 TO 15
100 POKE 53282, C
110 FOR D = 1 TO 100 : NEXT D
120 NEXT C
130 GOTO 90
```

---

## Mixing Hi-Res and Multicolor

### Per-Character Control

You can mix modes on the same screen by controlling color memory:

```basic
10 POKE 53270, PEEK(53270) OR 16 : REM Enable multicolor capability
20 PRINT CHR$(147)
30 REM First character in hi-res (color 0-7)
40 POKE 1024, 1   : REM Letter A
50 POKE 55296, 1  : REM Hi-res mode, white
60 REM Second character in multicolor (color 8-15)
70 POKE 1025, 1   : REM Letter A
80 POKE 55296 + 1, 9 : REM Multicolor mode, white foreground
```

**Result:** Two identical characters, one in hi-res, one in multicolor, sitting side-by-side.

### Practical Uses

**Hi-res for text, multicolor for graphics:**
```basic
10 POKE 53270, PEEK(53270) OR 16
20 REM Display text in hi-res
30 PRINT "SCORE: ";
40 REM Set first 7 chars to hi-res
50 FOR I = 55296 TO 55296 + 6
60 POKE I, PEEK(I) AND 7 : REM Clear bit 3
70 NEXT I
80 REM Draw multicolor sprite-like character
90 POKE 1024 + 10, 60
100 POKE 55296 + 10, 15 : REM Multicolor mode
```

---

## Complete Working Programs

### Program 1: Multicolor Character Demo

```basic
10 REM Setup custom character set
20 POKE 52,48:POKE 56,48:CLR
30 GOSUB 1000 : REM Copy ROM characters
40 POKE 53272,(PEEK(53272)AND 240)+12
50 REM Enable multicolor
60 POKE 53270,PEEK(53270)OR 16
70 POKE 53281,0  : REM Background #0 = Black
80 POKE 53282,2  : REM Background #1 = Red
90 POKE 53283,7  : REM Background #2 = Yellow
100 REM Create custom character at position 60
110 FOR I=0 TO 7
120 READ A
130 POKE 12288+(60*8)+I,A
140 NEXT I
150 DATA 129,37,21,29,93,85,85,85
160 REM Display
170 PRINT CHR$(147)
180 PRINT CHR$(60);
190 POKE 55296,9 : REM Multicolor, white
200 END
1000 REM Copy ROM characters
1010 POKE 56334,PEEK(56334)AND 254
1020 POKE 1,PEEK(1)AND 251
1030 FOR I=0 TO 511
1040 POKE 12288+I,PEEK(53248+I)
1050 NEXT I
1060 POKE 1,PEEK(1)OR 4
1070 POKE 56334,PEEK(56334)OR 1
1080 RETURN
```

### Program 2: Multicolor Palette Demonstration

```basic
10 REM Enable multicolor mode
20 POKE 53270,PEEK(53270)OR 16
30 PRINT CHR$(147)
40 REM Fill screen with letter 'A' in multicolor
50 FOR I=0 TO 22
60 PRINT CHR$(65);
70 NEXT I
80 REM Set all to multicolor mode
90 FOR I=0 TO 22
100 POKE 55296+I,8
110 NEXT I
120 REM Cycle through color combinations
130 FOR C1=0 TO 15
140 POKE 53282,C1
150 FOR C2=0 TO 15
160 POKE 53283,C2
170 FOR D=1 TO 20:NEXT D
180 NEXT C2,C1
190 GOTO 130
```

---

## Comparison Tables

### Standard vs Multicolor Mode

| Feature | Standard Mode | Multicolor Mode |
|---------|--------------|-----------------|
| Horizontal resolution | 320 pixels | 160 pixels |
| Pixels per character | 8 wide | 4 wide (double-width) |
| Colors per character | 2 | 4 |
| Bit interpretation | 1 bit = 1 pixel | 2 bits = 1 pixel |
| Character color choices | 16 colors (0-15) | 8 colors (0-7) |
| Background color choices | 16 colors (0-15) | 16 colors (0-15) |
| Per-character control | Via color memory | Via color memory bit 3 |

### Color Source Priority

**Standard mode:**
1. Bit 0 → Screen color (53281)
2. Bit 1 → Character color (color RAM)

**Multicolor mode:**
1. Bit pair 00 → Screen color (53281)
2. Bit pair 01 → Background #1 (53282)
3. Bit pair 10 → Background #2 (53283)
4. Bit pair 11 → Character color (color RAM bits 0-2)

---

## Important Notes

### Note About Sprites vs Characters

**Sprite multicolor:** Bit pair 10 = sprite individual color
**Character multicolor:** Bit pair 10 = background #2 color

These are DIFFERENT. Don't confuse sprite multicolor mode with character multicolor mode.

### Color Memory Bit Usage

In multicolor mode, color memory (55296-56295) uses bits differently:

| Bit | Purpose |
|-----|---------|
| 0-2 | Character foreground color (8 colors: 0-7) |
| 3 | Multicolor mode flag (0=hi-res, 1=multicolor) |
| 4-7 | Unused (always read as 1) |

### Compatible with Programmable Characters

Multicolor mode works with both ROM and RAM character sets:

```basic
REM Use custom characters + multicolor mode
POKE 53272,(PEEK(53272)AND 240)+12  : REM Custom chars
POKE 53270,PEEK(53270)OR 16         : REM Multicolor on
```

---

## Common Mistakes and Solutions

### Mistake 1: Characters Look Wrong

**Symptom:** Character patterns don't look like expected in multicolor mode
**Cause:** Using hi-res character data in multicolor mode
**Solution:** Design characters specifically for multicolor (bit pairs, not individual bits)

### Mistake 2: Limited Foreground Colors

**Symptom:** Can only use colors 0-7 for character foreground
**Cause:** Only 3 bits available (bit 3 used for mode flag)
**Solution:** Use colors 8-15 as background colors via registers 53282/53283

### Mistake 3: Global Enable Forgotten

**Symptom:** Color memory set to 8-15 but characters still hi-res
**Cause:** Bit 4 of register 53270 not set
**Solution:** `POKE 53270,PEEK(53270)OR 16`

### Mistake 4: Wrong Bit Pair Calculation

**Symptom:** Colors appear in wrong positions
**Cause:** Miscalculated bit pair values
**Solution:** Use bit pair reference table; remember leftmost pixel = bits 7-6

---

## Quick Reference

### Enable Multicolor Mode

```basic
REM Full multicolor setup
POKE 53270,PEEK(53270)OR 16  : REM Enable capability
POKE 53281,X : REM Background #0
POKE 53282,Y : REM Background #1
POKE 53283,Z : REM Background #2
POKE 55296,8+C : REM Set char to multicolor, foreground=C (0-7)
```

### Disable Multicolor Mode

```basic
REM Return to standard hi-res
POKE 53270,PEEK(53270)AND 239
```

### Key Registers

| Register | Purpose | Values |
|----------|---------|--------|
| 53270 | Multicolor enable (bit 4) | +16 to enable |
| 53281 | Background #0 (screen) | 0-15 |
| 53282 | Background #1 | 0-15 |
| 53283 | Background #2 | 0-15 |
| 55296+ | Char color + mode flag | 0-7 (hi-res), 8-15 (multicolor) |

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for mode overview
- **Related:** See PROGRAMMABLE-CHARACTERS-REFERENCE.md for character creation
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for memory maps

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
