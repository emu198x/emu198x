# Sprites Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

Sprites are hardware-controlled movable objects that can be displayed independently of the background screen. The VIC-II chip provides **8 hardware sprites** (numbered 0-7), each capable of:
- **24×21 pixel size** (expandable to 48×42)
- **Independent positioning** anywhere on screen
- **Independent colors** (standard or multicolor mode)
- **Hardware collision detection** with other sprites and background
- **Priority control** (in front of or behind background)

**Key advantage:** Sprites move without redrawing the background, making them ideal for game characters, bullets, cursors, and animated objects.

---

## Sprite Specifications

### Standard Sprite Mode

- **Size:** 24 pixels wide × 21 pixels tall
- **Colors:** 1 foreground color + transparent background
- **Memory:** 63 bytes + 1 padding byte = 64 bytes per sprite
- **Total sprites:** 8 simultaneous sprites (0-7)

### Multicolor Sprite Mode

- **Size:** 24 pixels wide × 21 pixels tall (but pixels are double-width)
- **Effective resolution:** 12 double-wide pixels × 21 pixels
- **Colors:** 3 colors + transparent background
- **Memory:** Same 64 bytes, but bit pairs select colors
- **Color sources:** 2 shared colors + 1 individual color

---

## Sprite Memory Format

### Standard Sprite Structure

Each sprite requires **64 bytes** of memory:
- **Bytes 0-62:** Sprite data (63 bytes = 24 bits × 21 rows)
- **Byte 63:** Unused padding byte (required for alignment)

**Memory layout:**
```
Byte 0:  Row 0 (3 bytes = 24 bits)
Byte 1:  Row 0 continued
Byte 2:  Row 0 continued
Byte 3:  Row 1 (3 bytes = 24 bits)
...
Byte 60: Row 20
Byte 61: Row 20 continued
Byte 62: Row 20 continued
Byte 63: Padding (unused)
```

### Bit Pattern

Each bit represents one pixel:
- **1** = Foreground color (sprite color)
- **0** = Transparent (background shows through)

**Example row (3 bytes = 24 bits):**
```
Binary:     11111111 11000011 11111111
Hex:        FF       C3       FF
Visual:     ******** **    ** ********
```

---

## Sprite Pointers

### What Are Sprite Pointers?

Sprite pointers tell the VIC-II where in memory each sprite's definition is located. They are **single-byte values** stored at the end of screen memory.

**Default locations:** 2040-2047 (last 8 bytes of screen memory at 1024-2023)

| Sprite | Pointer Address | Default Value |
|--------|----------------|---------------|
| 0 | 2040 | 0 |
| 1 | 2041 | 0 |
| 2 | 2042 | 0 |
| 3 | 2043 | 0 |
| 4 | 2044 | 0 |
| 5 | 2045 | 0 |
| 6 | 2046 | 0 |
| 7 | 2047 | 0 |

### Calculating Sprite Data Location

**Formula:**
```
Sprite Data Address = (Pointer Value × 64) + (Current Bank × 16384)
```

**Example:**
```basic
REM Use Bank 0, sprite data at 832 (13 × 64)
POKE 2040, 13  : REM Sprite 0 points to block 13
```

**Sprite data will be at:** 832 + (0 × 16384) = 832

### Pointer Value Range

**Valid pointer values:** 0-255
- Each value points to a 64-byte block within the current VIC-II bank
- Block 0 = offset 0
- Block 1 = offset 64
- Block 2 = offset 128
- Block 255 = offset 16320

**Avoid blocks that overlap:**
- Screen memory (typically blocks 16-31 in Bank 0)
- Character memory (blocks 16-31 or 48-63 in Banks 0 and 2)
- Your BASIC program

**Safe locations in Bank 0 (default):**
- Blocks 13-15 (832-1023) - Between screen and BASIC program start
- Blocks 192-254 (12288-16256) - If memory is protected with `POKE 52,48:POKE 56,48:CLR`

---

## Enabling and Positioning Sprites

### Enable Sprites

**Register:** 53269 ($D015) - Sprite enable register

Each bit controls one sprite:
- Bit 0 = Sprite 0
- Bit 1 = Sprite 1
- ...
- Bit 7 = Sprite 7

```basic
REM Enable sprite 0
POKE 53269, PEEK(53269) OR 1

REM Enable sprite 1
POKE 53269, PEEK(53269) OR 2

REM Enable sprites 0, 1, and 2
POKE 53269, PEEK(53269) OR 7  : REM 7 = binary 00000111

REM Enable all 8 sprites
POKE 53269, 255
```

### Sprite Positioning

**X Position:**
- Registers: 53248-53264 (even addresses for X position)
- Sprite 0 X: 53248
- Sprite 1 X: 53250
- Sprite 2 X: 53252
- ...
- Sprite 7 X: 53262

**Y Position:**
- Registers: 53249-53265 (odd addresses for Y position)
- Sprite 0 Y: 53249
- Sprite 1 Y: 53251
- Sprite 2 Y: 53253
- ...
- Sprite 7 Y: 53263

**X MSB (Most Significant Bit):**
- Register: 53264 ($D010)
- X position is 9 bits (0-511), but registers only hold 8 bits (0-255)
- Bit in 53264 provides the 9th bit for horizontal positions > 255

```basic
REM Position sprite 0 at X=100, Y=50
POKE 53248, 100  : REM X position
POKE 53249, 50   : REM Y position

REM Position sprite 0 at X=300, Y=100 (requires MSB)
POKE 53248, 44   : REM Low 8 bits of 300 = 44
POKE 53264, PEEK(53264) OR 1  : REM Set bit 0 (sprite 0 MSB)
POKE 53249, 100  : REM Y position
```

**Coordinate system:**
```
Screen visible area approximately:
X: 24-343 (actual values depend on screen mode)
Y: 50-249
```

---

## Sprite Colors

### Standard Sprite Colors

**Registers:** 53287-53294 ($D027-$D02E)

Each sprite has its own color register:
- Sprite 0: 53287
- Sprite 1: 53288
- Sprite 2: 53289
- Sprite 3: 53290
- Sprite 4: 53291
- Sprite 5: 53292
- Sprite 6: 53293
- Sprite 7: 53294

```basic
REM Set sprite 0 to white (color 1)
POKE 53287, 1

REM Set sprite 1 to red (color 2)
POKE 53288, 2
```

**Color codes:** 0-15 (same as character colors)

| Code | Color | Code | Color |
|------|-------|------|-------|
| 0 | Black | 8 | Orange |
| 1 | White | 9 | Brown |
| 2 | Red | 10 | Light Red |
| 3 | Cyan | 11 | Gray 1 |
| 4 | Purple | 12 | Gray 2 |
| 5 | Green | 13 | Light Green |
| 6 | Blue | 14 | Light Blue |
| 7 | Yellow | 15 | Gray 3 |

---

## Multicolor Sprite Mode

### Enabling Multicolor Mode

**Register:** 53276 ($D01C) - Multicolor sprite enable

Each bit controls one sprite:
```basic
REM Enable multicolor for sprite 0
POKE 53276, PEEK(53276) OR 1

REM Enable multicolor for sprites 0 and 1
POKE 53276, PEEK(53276) OR 3
```

### Color Sources in Multicolor Mode

In multicolor mode, bit pairs determine colors:

| Bit Pair | Color Source | Register/Location |
|----------|-------------|-------------------|
| 00 | Transparent | Background shows through |
| 01 | Sprite multicolor 0 (shared) | 53285 ($D025) |
| 10 | Sprite color (individual) | 53287-53294 |
| 11 | Sprite multicolor 1 (shared) | 53286 ($D026) |

**Setup example:**
```basic
REM Set shared multicolor registers
POKE 53285, 2  : REM Multicolor 0 = red (bit pair 01)
POKE 53286, 7  : REM Multicolor 1 = yellow (bit pair 11)

REM Set individual sprite color
POKE 53287, 1  : REM Sprite 0 individual color = white (bit pair 10)

REM Enable multicolor mode for sprite 0
POKE 53276, PEEK(53276) OR 1
```

### Multicolor Sprite Data Format

Bit pairs are read left to right:
```
Byte pattern:    11 01 10 00  (4 double-wide pixels)
Binary:          11010100
Decimal:         212
Colors:          MC1 MC0 IND TRANS
```

**Trade-off:** Half the horizontal resolution (12 double-wide pixels instead of 24 single pixels), but 4 colors instead of 2.

---

## Sprite Expansion

### Horizontal Expansion (2X Width)

**Register:** 53277 ($D01D) - Sprite X expansion

```basic
REM Expand sprite 0 horizontally (24 pixels → 48 pixels)
POKE 53277, PEEK(53277) OR 1
```

### Vertical Expansion (2X Height)

**Register:** 53271 ($D017) - Sprite Y expansion

```basic
REM Expand sprite 0 vertically (21 pixels → 42 pixels)
POKE 53271, PEEK(53271) OR 1
```

### Both Directions

```basic
REM Expand sprite 0 to 48×42 pixels
POKE 53277, PEEK(53277) OR 1  : REM X expansion
POKE 53271, PEEK(53271) OR 1  : REM Y expansion
```

**Result:** Standard sprite becomes 48×42 pixels, multicolor sprite becomes 24×42 effective pixels (48×42 physical with double-wide pixels)

---

## Sprite Priority

### Display Priority Register

**Register:** 53275 ($D01B) - Sprite-to-background priority

Controls whether sprites appear in front of or behind background:
- **Bit = 0:** Sprite in front of background (default)
- **Bit = 1:** Sprite behind background

```basic
REM Put sprite 0 behind background
POKE 53275, PEEK(53275) OR 1

REM Put sprite 1 behind background
POKE 53275, PEEK(53275) OR 2
```

**Use case:** Create depth effects (character walks behind trees, under bridges, etc.)

---

## Collision Detection

### Sprite-to-Sprite Collisions

**Register:** 53278 ($D01E) - Sprite collision register

Each bit indicates which sprites have collided with each other:
```basic
REM Check if any sprite-to-sprite collisions occurred
C = PEEK(53278)
IF C AND 1 THEN PRINT "SPRITE 0 HIT ANOTHER SPRITE"
IF C AND 2 THEN PRINT "SPRITE 1 HIT ANOTHER SPRITE"
```

**Important:** Reading this register clears it. Save the value before testing.

### Sprite-to-Background Collisions

**Register:** 53279 ($D01F) - Sprite-background collision register

Each bit indicates which sprites have collided with background graphics:
```basic
REM Check if sprite 0 hit background
C = PEEK(53279)
IF C AND 1 THEN PRINT "SPRITE 0 HIT BACKGROUND"
```

**Background** means any non-transparent pixel in character or bitmap mode.

**Clearing collision registers:**
```basic
REM Collision registers auto-clear on read, but you must read them
DUMMY = PEEK(53278)  : REM Clear sprite-sprite
DUMMY = PEEK(53279)  : REM Clear sprite-background
```

---

## Creating Sprite Data

### Design Grid

Each sprite is 24×21 pixels. Design on a grid:

```
Bit position (left to right in each byte):
Byte 1:    7 6 5 4 3 2 1 0
Byte 2:    7 6 5 4 3 2 1 0
Byte 3:    7 6 5 4 3 2 1 0
```

### Calculating Byte Values

**Example row (simple crosshair):**
```
Visual:     ........ ....**.. ........
Binary:     00000000 00001100 00000000
Decimal:    0        12       0
```

### Simple Sprite Example

**Solid square (8×8 in center):**
```basic
10 REM Define sprite at block 13 (address 832)
20 FOR I = 832 TO 894
30 READ A
40 POKE I, A
50 NEXT I
60 REM Top 7 rows blank
70 DATA 0,0,0, 0,0,0, 0,0,0, 0,0,0, 0,0,0, 0,0,0, 0,0,0
80 REM 8 rows with centered 8×8 square
90 DATA 0,255,0, 0,255,0, 0,255,0, 0,255,0
100 DATA 0,255,0, 0,255,0, 0,255,0, 0,255,0
110 REM Bottom 6 rows blank
120 DATA 0,0,0, 0,0,0, 0,0,0, 0,0,0, 0,0,0, 0,0,0
130 DATA 0  : REM Padding byte
```

---

## Complete Sprite Example

### Example: Display Moving Sprite

```basic
10 REM Define sprite data
20 FOR I=832 TO 894:READ A:POKE I,A:NEXT I
30 REM Point sprite 0 to block 13
40 POKE 2040,13
50 REM Set sprite color to white
60 POKE 53287,1
70 REM Enable sprite 0
80 POKE 53269,1
90 REM Move sprite across screen
100 FOR X=0 TO 255
110 POKE 53248,X  :REM X position
120 POKE 53249,100:REM Y position
130 FOR D=1 TO 10:NEXT D:REM Delay
140 NEXT X
150 GOTO 150
160 REM Sprite data: Simple arrow
170 DATA 0,16,0,0,56,0,0,124,0,0,254,0
180 DATA 1,255,0,3,255,128,7,255,192,15,255,224
190 DATA 1,255,0,0,254,0,0,124,0,0,56,0
200 DATA 0,16,0,0,0,0,0,0,0,0,0,0
210 DATA 0,0,0,0,0,0,0,0,0,0
```

---

## Sprite Register Quick Reference

### Position Registers

| Sprite | X Position | Y Position |
|--------|-----------|-----------|
| 0 | 53248 | 53249 |
| 1 | 53250 | 53251 |
| 2 | 53252 | 53253 |
| 3 | 53254 | 53255 |
| 4 | 53256 | 53257 |
| 5 | 53258 | 53259 |
| 6 | 53260 | 53261 |
| 7 | 53262 | 53263 |

### Control Registers

| Register | Decimal | Hex | Function |
|----------|---------|-----|----------|
| X MSB | 53264 | $D010 | 9th bit of X position (bit per sprite) |
| Enable | 53269 | $D015 | Sprite enable (bit per sprite) |
| Y Expansion | 53271 | $D017 | Vertical 2X expansion (bit per sprite) |
| Priority | 53275 | $D01B | Sprite-background priority (bit per sprite) |
| Multicolor | 53276 | $D01C | Multicolor enable (bit per sprite) |
| X Expansion | 53277 | $D01D | Horizontal 2X expansion (bit per sprite) |

### Color Registers

| Register | Decimal | Hex | Function |
|----------|---------|-----|----------|
| MC 0 | 53285 | $D025 | Multicolor shared color 0 |
| MC 1 | 53286 | $D026 | Multicolor shared color 1 |
| Sprite 0 | 53287 | $D027 | Sprite 0 individual color |
| Sprite 1 | 53288 | $D028 | Sprite 1 individual color |
| Sprite 2 | 53289 | $D029 | Sprite 2 individual color |
| Sprite 3 | 53290 | $D02A | Sprite 3 individual color |
| Sprite 4 | 53291 | $D02B | Sprite 4 individual color |
| Sprite 5 | 53292 | $D02C | Sprite 5 individual color |
| Sprite 6 | 53293 | $D02D | Sprite 6 individual color |
| Sprite 7 | 53294 | $D02E | Sprite 7 individual color |

### Collision Registers

| Register | Decimal | Hex | Function |
|----------|---------|-----|----------|
| Sprite-Sprite | 53278 | $D01E | Sprite collision flags (clears on read) |
| Sprite-Background | 53279 | $D01F | Background collision flags (clears on read) |

---

## Easy Spritemaking Chart

### Quick Reference for All 8 Sprites

This table shows pointer values, memory addresses, and DATA statement ranges for setting up all 8 sprites in common locations.

**Using Blocks 13-15 (Before BASIC Program):**

| Sprite | Pointer Value | Memory Address | DATA Statement Range |
|--------|---------------|----------------|---------------------|
| 0 | 13 | 832-894 | Lines 100-120 |
| 1 | 14 | 896-958 | Lines 200-220 |
| 2 | 15 | 960-1022 | Lines 300-320 |
| 3-7 | — | (requires protected memory) | — |

**Using Protected Memory (POKE 52,48:POKE 56,48:CLR):**

| Sprite | Pointer Value | Memory Address | DATA Statement Range |
|--------|---------------|----------------|---------------------|
| 0 | 192 | 12288-12350 | Lines 1000-1020 |
| 1 | 193 | 12352-12414 | Lines 1100-1120 |
| 2 | 194 | 12416-12478 | Lines 1200-1220 |
| 3 | 195 | 12480-12542 | Lines 1300-1320 |
| 4 | 196 | 12544-12606 | Lines 1400-1420 |
| 5 | 197 | 12608-12670 | Lines 1500-1520 |
| 6 | 198 | 12672-12734 | Lines 1600-1620 |
| 7 | 199 | 12736-12798 | Lines 1700-1720 |

**Setup pattern example:**
```basic
10 REM Protect memory for sprites
20 POKE 52,48:POKE 56,48:CLR
30 REM Set sprite pointers
40 POKE 2040,192:POKE 2041,193:POKE 2042,194
50 REM Load sprite data
60 FOR I=12288 TO 12350:READ A:POKE I,A:NEXT I
70 REM Enable sprites
80 POKE 53269,7:REM Enable sprites 0,1,2
```

**Notes:**
- Each sprite requires 63 bytes of data + 1 padding byte = 64 bytes total
- Pointer value × 64 = memory address within current VIC-II bank
- DATA statement ranges are approximate - adjust based on your program structure
- Always include the padding byte (value 0) as the 64th byte

---

## Memory Layout Examples

### Example 1: Bank 0, Default Screen

```
Screen memory: 1024-2023
Sprite pointers: 2040-2047
Sprite data blocks 13-15: 832-1023 (safe, before BASIC)

Setup:
POKE 2040, 13  : REM Sprite 0 at 832
POKE 2041, 14  : REM Sprite 1 at 896
POKE 2042, 15  : REM Sprite 2 at 960
```

### Example 2: Bank 0, Protected Memory

```
BASIC program: 2048-12287
Protected: 12288+ (POKE 52,48:POKE 56,48:CLR)
Sprite data blocks 192-254: 12288-16256

Setup:
POKE 2040, 192  : REM Sprite 0 at 12288
POKE 2041, 193  : REM Sprite 1 at 12352
```

---

## Common Sprite Patterns

### Pattern 1: Animated Sprite

```basic
10 REM Animate sprite with 4 frames
20 FRAME=13:REM Starting block
30 POKE 2040,FRAME
40 FOR D=1 TO 50:NEXT D
50 FRAME=FRAME+1
60 IF FRAME>16 THEN FRAME=13
70 GOTO 30
```

### Pattern 2: Sprite Following Cursor

```basic
10 REM Sprite follows joystick
20 X=160:Y=100
30 J=PEEK(56320)
40 IF J AND 1 THEN Y=Y-1:REM Up
50 IF J AND 2 THEN Y=Y+1:REM Down
60 IF J AND 4 THEN X=X-1:REM Left
70 IF J AND 8 THEN X=X+1:REM Right
80 POKE 53248,X:POKE 53249,Y
90 GOTO 30
```

### Pattern 3: Collision Response

```basic
10 REM Check collisions each frame
20 C=PEEK(53278):REM Sprite-sprite
30 IF C AND 1 THEN GOSUB 1000:REM Sprite 0 hit
40 C=PEEK(53279):REM Sprite-background
50 IF C AND 1 THEN GOSUB 2000:REM Sprite 0 hit wall
60 GOTO 20
```

---

## Important Notes

### Sprite Pointer Location Changes

If you relocate screen memory, sprite pointers move:
```
Sprite pointers always at: Screen Memory + 1016 to Screen Memory + 1023
```

**Example:**
```basic
REM Screen at 2048 (instead of 1024)
REM Sprite pointers now at 3064-3071
POKE 3064, 13  : REM Sprite 0 pointer
```

### Sprite Data Alignment

Sprite data MUST start at addresses that are multiples of 64. The pointer value × 64 gives the address within the current bank.

**Valid addresses:** 0, 64, 128, 192, 256, 320, ..., 16320

**Invalid addresses:** 100, 150, 1000 (not multiples of 64)

### VIC-II Bank Limitation

Sprites can only use data within the current 16K VIC-II bank. If you switch banks, sprite definitions must be in the new bank.

### Performance Consideration

Hardware sprites are MUCH faster than software sprites (moving characters on screen). Use them whenever possible for smooth animation.

---

## Troubleshooting

### Sprite Doesn't Appear

**Checklist:**
1. Is sprite enabled? Check 53269
2. Is sprite on screen? X: 24-343, Y: 50-249
3. Is sprite pointer set? Check 2040-2047
4. Does pointer point to valid data? Value × 64 = data address
5. Is sprite data defined? Check memory at pointer location
6. Is sprite color different from background? Check 53287+

### Sprite Appears Corrupted

**Causes:**
1. Sprite data corrupted by BASIC variables (protect memory)
2. Pointer value incorrect (points to random memory)
3. Incomplete sprite data (less than 63 bytes defined)

### Collision Detection Not Working

**Fixes:**
1. Read collision registers each frame
2. Remember: Reading clears the register
3. Save value before testing bits
4. Check sprite priority (background collisions don't detect if sprite is behind)

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for graphics mode details
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for memory organization

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
