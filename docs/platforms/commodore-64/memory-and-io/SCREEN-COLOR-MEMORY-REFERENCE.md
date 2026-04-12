# C64 Screen and Color Memory Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Official Commodore 64 Programmer's Reference Guide, Appendix D & G

---

## Overview

The C64 screen is controlled by two separate memory regions:
- **Screen Memory (1024-2023)** - Controls WHAT character is displayed
- **Color Memory (55296-56295)** - Controls WHAT COLOR each character is

Each location in screen memory has a corresponding location in color memory at the same relative offset.

---

## Memory Ranges

| Memory Type | Start | End | Size | Purpose |
|-------------|-------|-----|------|---------|
| Screen Memory | 1024 | 2023 | 1000 bytes | Character display codes |
| Color Memory | 55296 | 56295 | 1000 bytes | Character color codes (0-15) |

**Screen Layout:** 40 columns × 25 rows = 1000 character positions

---

## Screen Memory Map (1024-2023)

### Row Start Addresses

| Row | Start Address | End Address | Formula |
|-----|---------------|-------------|---------|
| 0 | 1024 | 1063 | 1024 + (0 × 40) |
| 1 | 1064 | 1103 | 1024 + (1 × 40) |
| 2 | 1104 | 1143 | 1024 + (2 × 40) |
| 3 | 1144 | 1183 | 1024 + (3 × 40) |
| 4 | 1184 | 1223 | 1024 + (4 × 40) |
| 5 | 1224 | 1263 | 1024 + (5 × 40) |
| 6 | 1264 | 1303 | 1024 + (6 × 40) |
| 7 | 1304 | 1343 | 1024 + (7 × 40) |
| 8 | 1344 | 1383 | 1024 + (8 × 40) |
| 9 | 1384 | 1423 | 1024 + (9 × 40) |
| 10 | 1424 | 1463 | 1024 + (10 × 40) |
| 11 | 1464 | 1503 | 1024 + (11 × 40) |
| 12 | 1504 | 1543 | 1024 + (12 × 40) |
| 13 | 1544 | 1583 | 1024 + (13 × 40) |
| 14 | 1584 | 1623 | 1024 + (14 × 40) |
| 15 | 1624 | 1663 | 1024 + (15 × 40) |
| 16 | 1664 | 1703 | 1024 + (16 × 40) |
| 17 | 1704 | 1743 | 1024 + (17 × 40) |
| 18 | 1744 | 1783 | 1024 + (18 × 40) |
| 19 | 1784 | 1823 | 1024 + (19 × 40) |
| 20 | 1824 | 1863 | 1024 + (20 × 40) |
| 21 | 1864 | 1903 | 1024 + (21 × 40) |
| 22 | 1904 | 1943 | 1024 + (22 × 40) |
| 23 | 1944 | 1983 | 1024 + (23 × 40) |
| 24 | 1984 | 2023 | 1024 + (24 × 40) |

**Position Formula:**
```
Screen Address = 1024 + (Row × 40) + Column
```

Where Row = 0-24, Column = 0-39

---

## Color Memory Map (55296-56295)

### Row Start Addresses

| Row | Start Address | End Address | Formula |
|-----|---------------|-------------|---------|
| 0 | 55296 | 55335 | 55296 + (0 × 40) |
| 1 | 55336 | 55375 | 55296 + (1 × 40) |
| 2 | 55376 | 55415 | 55296 + (2 × 40) |
| 3 | 55416 | 55455 | 55296 + (3 × 40) |
| 4 | 55456 | 55495 | 55296 + (4 × 40) |
| 5 | 55496 | 55535 | 55296 + (5 × 40) |
| 6 | 55536 | 55575 | 55296 + (6 × 40) |
| 7 | 55576 | 55615 | 55296 + (7 × 40) |
| 8 | 55616 | 55655 | 55296 + (8 × 40) |
| 9 | 55656 | 55695 | 55296 + (9 × 40) |
| 10 | 55696 | 55735 | 55296 + (10 × 40) |
| 11 | 55736 | 55775 | 55296 + (11 × 40) |
| 12 | 55776 | 55815 | 55296 + (12 × 40) |
| 13 | 55816 | 55855 | 55296 + (13 × 40) |
| 14 | 55856 | 55895 | 55296 + (14 × 40) |
| 15 | 55896 | 55935 | 55296 + (15 × 40) |
| 16 | 55936 | 55975 | 55296 + (16 × 40) |
| 17 | 55976 | 56015 | 55296 + (17 × 40) |
| 18 | 56016 | 56055 | 55296 + (18 × 40) |
| 19 | 56056 | 56095 | 55296 + (19 × 40) |
| 20 | 56096 | 56135 | 55296 + (20 × 40) |
| 21 | 56136 | 56175 | 55296 + (21 × 40) |
| 22 | 56176 | 56215 | 55296 + (22 × 40) |
| 23 | 56216 | 56255 | 55296 + (23 × 40) |
| 24 | 56256 | 56295 | 55296 + (24 × 40) |

**Position Formula:**
```
Color Address = 55296 + (Row × 40) + Column
```

Where Row = 0-24, Column = 0-39

---

## Color Codes

### Character Color Memory (55296-56295)

| Code | Hex | Color | Code | Hex | Color |
|------|-----|-------|------|-----|-------|
| 0 | $00 | Black | 8 | $08 | Orange |
| 1 | $01 | White | 9 | $09 | Brown |
| 2 | $02 | Red | 10 | $0A | Light Red |
| 3 | $03 | Cyan | 11 | $0B | Gray 1 (Dark Gray) |
| 4 | $04 | Purple | 12 | $0C | Gray 2 (Medium Gray) |
| 5 | $05 | Green | 13 | $0D | Light Green |
| 6 | $06 | Blue | 14 | $0E | Light Blue |
| 7 | $07 | Yellow | 15 | $0F | Gray 3 (Light Gray) |

---

## VIC-II Color Registers (53280+)

### Border and Background Colors

| Register | Decimal | Hex | Purpose |
|----------|---------|-----|---------|
| 32 | 53280 | $D020 | Border color |
| 33 | 53281 | $D021 | Background color 0 (main background) |
| 34 | 53282 | $D022 | Background color 1 (multicolor mode) |
| 35 | 53283 | $D023 | Background color 2 (multicolor mode) |
| 36 | 53284 | $D024 | Background color 3 (multicolor mode) |

### Sprite Colors

| Register | Decimal | Hex | Purpose |
|----------|---------|-----|---------|
| 37 | 53285 | $D025 | Sprite multicolor 0 (shared) |
| 38 | 53286 | $D026 | Sprite multicolor 1 (shared) |
| 39 | 53287 | $D027 | Sprite 0 color |
| 40 | 53288 | $D028 | Sprite 1 color |
| 41 | 53289 | $D029 | Sprite 2 color |
| 42 | 53290 | $D02A | Sprite 3 color |
| 43 | 53291 | $D02B | Sprite 4 color |
| 44 | 53292 | $D02C | Sprite 5 color |
| 45 | 53293 | $D02D | Sprite 6 color |
| 46 | 53294 | $D02E | Sprite 7 color |

**Important:** Only colors 0-7 may be used in multicolor character mode.

---

## Practical Examples

### Example 1: Display Red "A" at Top-Left

```basic
10 REM Display "A" (screen code 1) in red (color 2)
20 POKE 1024,1      : REM Character "A"
30 POKE 55296,2     : REM Red color
```

### Example 2: Display Character at Row 10, Column 15

```basic
10 REM Calculate position
20 ROW=10:COL=15
30 SCRN=1024+(ROW*40)+COL
40 COLR=55296+(ROW*40)+COL
50 REM Display yellow circle
60 POKE SCRN,81     : REM Circle character
70 POKE COLR,7      : REM Yellow color
```

### Example 3: Clear Screen to Spaces

```basic
10 REM Clear entire screen
20 FOR I=1024 TO 2023
30 POKE I,32        : REM Space character
40 NEXT I
```

### Example 4: Set All Characters to Blue

```basic
10 REM Color entire screen blue
20 FOR I=55296 TO 56295
30 POKE I,6         : REM Blue
40 NEXT I
```

### Example 5: Draw Horizontal Line at Row 12

```basic
10 REM Horizontal line across middle of screen
20 ROW=12
30 FOR COL=0 TO 39
40 SCRN=1024+(ROW*40)+COL
50 COLR=55296+(ROW*40)+COL
60 POKE SCRN,67     : REM Horizontal line character
70 POKE COLR,1      : REM White
80 NEXT COL
```

### Example 6: Create Rainbow Border Effect

```basic
10 REM Cycle through border colors
20 FOR C=0 TO 15
30 POKE 53280,C     : REM Border color
40 FOR D=1 TO 100:NEXT D  : REM Delay
50 NEXT C
60 GOTO 20          : REM Repeat
```

### Example 7: Display Text with Individual Colors

```basic
10 REM "HELLO" with each letter different color
20 DATA 8,5,12,12,15        : REM Screen codes
30 DATA 2,7,5,6,3           : REM Colors (red,yellow,green,blue,cyan)
40 FOR I=0 TO 4
50 READ C
60 POKE 1024+I,C            : REM Character
70 NEXT I
80 FOR I=0 TO 4
90 READ C
100 POKE 55296+I,C          : REM Color
110 NEXT I
```

### Example 8: Flash Character (Alternate Colors)

```basic
10 REM Flash character at position 1504
20 POKE 1504,81             : REM Circle character
30 FOR F=1 TO 10
40 POKE 55776,1             : REM White
50 FOR D=1 TO 100:NEXT D    : REM Delay
60 POKE 55776,0             : REM Black
70 FOR D=1 TO 100:NEXT D    : REM Delay
80 NEXT F
```

### Example 9: Convert Screen Coordinates to Address

```basic
10 REM Screen position calculator
20 INPUT "ROW (0-24)"; R
30 INPUT "COLUMN (0-39)"; C
40 S=1024+(R*40)+C
50 CL=55296+(R*40)+C
60 PRINT "SCREEN ADDR:";S
70 PRINT "COLOR ADDR:";CL
```

### Example 10: Center Text on Screen

```basic
10 REM Center text on row 12
20 T$="GAME OVER"
30 L=LEN(T$)
40 COL=(40-L)/2            : REM Center column
50 ROW=12
60 FOR I=1 TO L
70 A$=MID$(T$,I,1)
80 C=ASC(A$)-64            : REM Convert to screen code
90 SCRN=1024+(ROW*40)+(COL+I-1)
100 COLR=55296+(ROW*40)+(COL+I-1)
110 POKE SCRN,C
120 POKE COLR,1            : REM White
130 NEXT I
```

---

## Common Position Shortcuts

### Screen Center
```basic
ADDR=1024+(12*40)+20  : REM Row 12, Column 20 (center)
REM = 1024+480+20 = 1524
```

### Top-Left Corner
```basic
ADDR=1024  : REM Row 0, Column 0
```

### Top-Right Corner
```basic
ADDR=1063  : REM Row 0, Column 39
```

### Bottom-Left Corner
```basic
ADDR=1984  : REM Row 24, Column 0
```

### Bottom-Right Corner
```basic
ADDR=2023  : REM Row 24, Column 39
```

---

## Memory Layout Visualization

```
Screen Memory Layout (1024-2023):

Column →  0   1   2  ...  38  39
        ┌──────────────────────────┐
Row 0   │1024              ...  1063│
Row 1   │1064              ...  1103│
Row 2   │1104              ...  1143│
...     │ ...                   ... │
Row 12  │1504 (CENTER)      ... 1543│
...     │ ...                   ... │
Row 24  │1984              ...  2023│
        └──────────────────────────┘

Color Memory Layout (55296-56295):

Column →  0    1    2  ...   38   39
        ┌────────────────────────────┐
Row 0   │55296             ... 55335│
Row 1   │55336             ... 55375│
Row 2   │55376             ... 55415│
...     │  ...                  ...  │
Row 12  │55776 (CENTER)    ... 55815│
...     │  ...                  ...  │
Row 24  │56256             ... 56295│
        └────────────────────────────┘
```

---

## Important Notes

### Color Memory Quirks

1. **Only 4 bits used** - Color memory only uses bits 0-3 (values 0-15)
2. **Upper bits undefined** - Bits 4-7 are not connected and may contain garbage
3. **Always mask when reading** - Use `AND 15` when reading color memory
   ```basic
   C=PEEK(55296) AND 15  : REM Ensure only valid color bits
   ```

### Screen Memory vs PETSCII

- Screen memory uses **screen codes** (0-255)
- PRINT uses **PETSCII codes** (different mapping)
- Screen code 1 = "A", but PETSCII code 65 = "A"
- Always convert PETSCII to screen codes when POKEing

### Performance Tips

1. **Batch updates** - Update multiple positions in one loop
2. **Avoid unnecessary color changes** - Color memory writes are slower
3. **Use DATA statements** - Pre-calculate positions for static graphics
4. **Store frequently used addresses** - Calculate once, reuse

---

## Quick Reference Card

```
SCREEN MEMORY
Start:  1024
End:    2023
Size:   1000 bytes
Layout: 40 columns × 25 rows

COLOR MEMORY
Start:  55296
End:    56295
Size:   1000 bytes
Values: 0-15 only

POSITION FORMULAS
Screen: 1024 + (Row × 40) + Column
Color:  55296 + (Row × 40) + Column

BORDER/BACKGROUND
Border:     53280 (POKE 53280,color)
Background: 53281 (POKE 53281,color)

COMMON POSITIONS
Top-Left:     1024 / 55296
Top-Right:    1063 / 55335
Center:       1524 / 55796
Bottom-Left:  1984 / 56256
Bottom-Right: 2023 / 56295

COLOR CODES
0=Black   4=Purple   8=Orange    12=Gray 2
1=White   5=Green    9=Brown     13=Lt Green
2=Red     6=Blue    10=Lt Red    14=Lt Blue
3=Cyan    7=Yellow  11=Gray 1    15=Gray 3
```

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Appendix D & G
- **Related:** See SCREEN-CODES-REFERENCE.md for character codes
- **Related:** See VIC-CHIP-REFERENCE.md for VIC-II registers

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
