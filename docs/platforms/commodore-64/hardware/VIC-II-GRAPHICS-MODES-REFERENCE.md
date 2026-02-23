# VIC-II Graphics Modes Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

All C64 graphics capabilities come from the **6567 Video Interface Chip (VIC-II)**. This chip provides:
- 40×25 text display (1000 character positions)
- 320×200 high-resolution graphics
- Sprites (movable objects)
- Multiple graphics modes that can be mixed on the same screen

---

## Graphics Mode Categories

### A. CHARACTER DISPLAY MODES

1. **Standard Character Mode**
   - ROM characters (default)
   - RAM programmable characters

2. **Multicolor Character Mode**
   - ROM characters
   - RAM programmable characters
   - 4 colors per character, reduced horizontal resolution

3. **Extended Background Color Mode**
   - ROM characters
   - RAM programmable characters
   - 4 different background colors

### B. BITMAP MODES

1. **Standard Bitmap Mode** (320×200, 2 colors per 8×8 cell)
2. **Multicolor Bitmap Mode** (160×200, 4 colors per 8×8 cell)

### C. SPRITES

1. **Standard Sprites** (24×21, 2 colors)
2. **Multicolor Sprites** (24×21, 4 colors)

---

## Memory Bank Selection

The VIC-II chip can only "see" **16K of memory at a time**. The C64's 64K is divided into four 16K banks.

### Bank Select Register

**Location:** 56576 ($DD00) - CIA#2 Port A, bits 0-1
**Data Direction:** 56578 ($DD02) - must set bits 0-1 to output

### Bank Selection Code

```basic
REM Set bits 0 and 1 as outputs
POKE 56578, PEEK(56578) OR 3

REM Select bank (A = 0-3)
POKE 56576, (PEEK(56576) AND 252) OR A
```

### Bank Table

| Value of A | Bits | Bank | Starting Location | VIC-II Range |
|------------|------|------|-------------------|--------------|
| 0 | 00 | 3 | 49152 | $C000-$FFFF |
| 1 | 01 | 2 | 32768 | $8000-$BFFF |
| 2 | 10 | 1 | 16384 | $4000-$7FFF |
| 3 | 11 | 0 | 0 | $0000-$3FFF (DEFAULT) |

**Important:** The C64 character ROM is only available in Banks 0 and 2.

---

## Screen Memory Location

**Default:** 1024-2023 ($0400-$07E7)
**Control Register:** 53272 ($D018), bits 4-7

### Screen Memory Relocation Code

```basic
REM Relocate screen memory
POKE 53272, (PEEK(53272) AND 15) OR A
```

### Screen Memory Addresses (within 16K bank)

| A | Bits | Decimal | Hex | Notes |
|---|------|---------|-----|-------|
| 0 | 0000XXXX | 0 | $0000 | |
| 16 | 0001XXXX | 1024 | $0400 | DEFAULT |
| 32 | 0010XXXX | 2048 | $0800 | |
| 48 | 0011XXXX | 3072 | $0C00 | |
| 64 | 0100XXXX | 4096 | $1000 | |
| 80 | 0101XXXX | 5120 | $1400 | |
| 96 | 0110XXXX | 6144 | $1800 | |
| 112 | 0111XXXX | 7168 | $1C00 | |
| 128 | 1000XXXX | 8192 | $2000 | |
| 144 | 1001XXXX | 9216 | $2400 | |
| 160 | 1010XXXX | 10240 | $2800 | |
| 176 | 1011XXXX | 11264 | $2C00 | |
| 192 | 1100XXXX | 12288 | $3000 | |
| 208 | 1101XXXX | 13312 | $3400 | |
| 224 | 1110XXXX | 14336 | $3800 | |
| 240 | 1111XXXX | 15360 | $3C00 | |

**Note:** Add bank base address to get absolute address. Must also notify KERNAL:
`POKE 648, page` where `page = address/256`

---

## Color Memory

**Location:** 55296-56295 ($D800-$DBE7) - FIXED, cannot be relocated
**Size:** 1000 bytes (4 bits per location, values 0-15)

Color memory is **always** at the same location regardless of screen memory or bank selection.

---

## Character Memory Location

Characters are defined by 8 bytes each (8×8 grid). A full character set requires **2048 bytes (2K)**.

**Control Register:** 53272 ($D018), bits 1-3 (bit 0 ignored)

### Character Memory Relocation Code

```basic
REM Point to character memory location
POKE 53272, (PEEK(53272) AND 240) OR A
```

### Character Memory Addresses (within 16K bank)

| A | Bits | Decimal | Hex | Notes |
|---|------|---------|-----|-------|
| 0 | XXXX000X | 0 | $0000-$07FF | |
| 2 | XXXX001X | 2048 | $0800-$0FFF | |
| 4 | XXXX010X | 4096 | $1000-$17FF | ROM IMAGE (Banks 0 & 2) |
| 6 | XXXX011X | 6144 | $1800-$1FFF | ROM IMAGE (Banks 0 & 2) |
| 8 | XXXX100X | 8192 | $2000-$27FF | |
| 10 | XXXX101X | 10240 | $2800-$2FFF | |
| 12 | XXXX110X | 12288 | $3000-$37FF | |
| 14 | XXXX111X | 14336 | $3800-$3FFF | |

**ROM Image:** Character ROM appears at locations $1000-$1FFF in Banks 0 and 2, even though it's physically at $D000-$DFFF.

---

## Character ROM Contents

**Physical Location:** 53248-57343 ($D000-$DFFF)
**Note:** Shares address space with I/O registers

### Character ROM Blocks

| Block | Address | Hex | VIC-II Image | Contents |
|-------|---------|-----|--------------|----------|
| 0 | 53248-53759 | D000-D1FF | 1000-11FF | Uppercase characters |
| 0 | 53760-54271 | D200-D3FF | 1200-13FF | Graphics characters |
| 0 | 54272-54783 | D400-D5FF | 1400-15FF | Reversed uppercase |
| 0 | 54784-55295 | D600-D7FF | 1600-17FF | Reversed graphics |
| 1 | 55296-55807 | D800-D9FF | 1800-19FF | Lowercase characters |
| 1 | 55808-56319 | DA00-DBFF | 1A00-1BFF | Uppercase & graphics |
| 1 | 56320-56831 | DC00-DDFF | 1C00-1DFF | Reversed lowercase |
| 1 | 56832-57343 | DE00-DFFF | 1E00-1FFF | Reversed uppercase & graphics |

### Accessing Character ROM

To copy characters from ROM, you must temporarily switch out I/O:

```basic
REM Turn off interrupts
POKE 56334, PEEK(56334) AND 254

REM Switch in character ROM
POKE 1, PEEK(1) AND 251

REM Copy character data here
REM Character ROM now at 53248-57343

REM Switch I/O back in
POKE 1, PEEK(1) OR 4

REM Turn interrupts back on
POKE 56334, PEEK(56334) OR 1
```

**Warning:** Do NOT allow interrupts while I/O is switched out, or the system will crash.

---

## Standard Character Mode

**Default mode** - The mode active when C64 powers on.

### How It Works

1. VIC-II reads screen memory (1024-2023) to get character code (0-255)
2. VIC-II reads color memory (55296-56295) to get character color (0-15)
3. VIC-II calculates character pattern address:
   ```
   ADDRESS = (CODE × 8) + (CHAR_SET × 2048) + (BANK × 16384)
   ```
4. VIC-II reads 8 bytes from character memory to get 8×8 bit pattern
5. For each bit: 0 = background color, 1 = character color

### Character Definition Format

Each character = 8 bytes, each byte = one row, each bit = one pixel.

**Example: Letter "A"**

```
Row  Binary    Decimal  Visual
0    00011000    24     **
1    00111100    60     ****
2    01100110   102     **  **
3    01111110   126     ******
4    01100110   102     **  **
5    01100110   102     **  **
6    01100110   102     **  **
7    00000000     0
```

---

## Multicolor Character Mode

**Control Register:** 53270 ($D016), bit 4

### Enable Multicolor Mode

```basic
REM Turn multicolor mode ON
POKE 53270, PEEK(53270) OR 16

REM Turn multicolor mode OFF
POKE 53270, PEEK(53270) AND 239
```

### How It Works

**Per-character activation:** Controlled by bit 3 of color memory (55296-56295)
- Color value 0-7: Standard hi-res mode
- Color value 8-15: Multicolor mode (bit 3 = 1)

**Bit pairs become color selectors:**

| Bit Pair | Color Source | Register | Location |
|----------|-------------|----------|----------|
| 00 | Background #0 (screen color) | - | 53281 ($D021) |
| 01 | Background #1 | Shared | 53282 ($D022) |
| 10 | Background #2 | Shared | 53283 ($D023) |
| 11 | Character color | Individual | Color RAM (bits 0-2) |

**Trade-off:** Horizontal resolution is halved (160 effective pixels vs 320), but you gain 4 colors per character instead of 2.

### Color Registers

```basic
REM Set multicolor background registers
POKE 53281, 0  : REM Background #0 (screen color)
POKE 53282, 2  : REM Background #1 (shared)
POKE 53283, 7  : REM Background #2 (shared)
```

**Character color** comes from lower 3 bits of color memory (8 colors: 0-7).

---

## Extended Background Color Mode

**Control Register:** 53265 ($D011), bit 6

### Enable Extended Background Color Mode

```basic
REM Turn extended background mode ON
POKE 53265, PEEK(53265) OR 64

REM Turn extended background mode OFF
POKE 53265, PEEK(53265) AND 191
```

### How It Works

Extended background color mode provides **4 different background colors** at the cost of reducing available characters to **64** (instead of 256).

**Character code interpretation:**
- **Bits 6-7** (upper 2 bits) select which background color to use
- **Bits 0-5** (lower 6 bits) select which of 64 character patterns to display

**Character code breakdown:**
```
Character Code: 11010110 (binary)
Bits 6-7: 11 = Background color #3
Bits 0-5: 010110 = Character pattern 22
```

### Background Color Selection

| Bits 6-7 | Background Color | Register |
|----------|-----------------|----------|
| 00 | Background #0 (screen color) | 53281 ($D021) |
| 01 | Background #1 | 53282 ($D022) |
| 10 | Background #2 | 53283 ($D023) |
| 11 | Background #3 | 53284 ($D024) |

**Foreground color** still comes from color memory (55296-56295).

### Setup Example

```basic
REM Enable extended background mode
POKE 53265, PEEK(53265) OR 64

REM Set the 4 background colors
POKE 53281, 0  : REM Background #0 = black
POKE 53282, 6  : REM Background #1 = blue
POKE 53283, 2  : REM Background #2 = red
POKE 53284, 5  : REM Background #3 = green

REM Display character 10 with background #2 (red)
REM Character code = (2 << 6) + 10 = 128 + 10 = 138
POKE 1024, 138      : REM Character with red background
POKE 55296, 1       : REM White foreground
```

### Character Code Formula

```basic
REM Calculate character code for extended mode
CODE = (BACKGROUND × 64) + CHARACTER
```

Where:
- BACKGROUND = 0-3 (which background color to use)
- CHARACTER = 0-63 (which of 64 patterns to display)

**Examples:**
```basic
REM Character 5 with background #0
CODE = (0 × 64) + 5 = 5

REM Character 5 with background #1
CODE = (1 × 64) + 5 = 69

REM Character 5 with background #2
CODE = (2 × 64) + 5 = 133

REM Character 5 with background #3
CODE = (3 × 64) + 5 = 197
```

### Character Set Limitation

**Only the first 64 characters** (0-63) from the character set are available.

In standard ROM character set:
- Characters 0-63 include: @, A-Z, [, \, ], ↑, ←, space, !, ", #, $, %, &, ', (, ), *, +, comma, -, ., /, 0-9, :, ;, <, =, >, ?

**Cannot access** characters 64-255 (lowercase, graphics symbols, etc.) while in extended background mode.

### Use Cases

**Ideal for:**
- Text with colored paragraphs or sections
- Highlighting text areas with different backgrounds
- Creating color-coded displays
- Menu systems with visual separation

**Not ideal for:**
- Games requiring full character set
- Applications needing lowercase letters
- Graphics requiring character set flexibility

### Complete Example

```basic
10 REM Extended background mode demo
20 POKE 53265, PEEK(53265) OR 64
30 REM Set 4 background colors
40 POKE 53281, 0  : REM Black
50 POKE 53282, 6  : REM Blue
60 POKE 53283, 2  : REM Red
70 POKE 53284, 5  : REM Green
80 REM Display text with different backgrounds
90 FOR I = 0 TO 39
100 REM Row 0: Background #0 (black)
110 POKE 1024 + I, 1 + I  : REM Characters A-Z
120 POKE 55296 + I, 1     : REM White text
130 REM Row 1: Background #1 (blue)
140 POKE 1064 + I, 64 + 1 + I
150 POKE 55336 + I, 7     : REM Yellow text
160 REM Row 2: Background #2 (red)
170 POKE 1104 + I, 128 + 1 + I
180 POKE 55376 + I, 1     : REM White text
190 REM Row 3: Background #3 (green)
200 POKE 1144 + I, 192 + 1 + I
210 POKE 55416 + I, 0     : REM Black text
220 NEXT I
```

### Important Notes

**Cannot combine with multicolor mode:** Extended background mode and multicolor character mode are mutually exclusive. Enabling one disables the other.

**Screen codes vs character patterns:** In extended mode, screen memory values 0-63 use pattern 0-63 with background #0, values 64-127 use patterns 0-63 with background #1, etc.

**Programmable characters:** Works with custom character sets in RAM. Only define characters 0-63 to save memory (512 bytes instead of 2048 bytes).

---

## Screen Blanking

**Control Register:** 53265 ($D011), bit 4

### Enable/Disable Screen Display

Screen blanking turns off the display without losing any data. The entire screen changes to border color.

```basic
REM Turn screen OFF (blank to border color)
POKE 53265, PEEK(53265) AND 239

REM Turn screen back ON
POKE 53265, PEEK(53265) OR 16
```

### Use Cases

**Performance boost:**
- Turning off the screen speeds up the processor slightly
- Useful during complex calculations or data loading
- Programs run faster when screen is blanked

**Visual effects:**
- Smooth transitions between screens
- Hide screen updates during setup
- Create fade-to-black effects (combined with border color change)

### Important Notes

**Data is preserved:** Screen memory, color memory, and all graphics data remain unchanged. Only the display is turned off.

**Border remains visible:** The border continues to display in its current color. To create a complete blackout:

```basic
REM Complete blackout
POKE 53280, 0  : REM Set border to black
POKE 53265, PEEK(53265) AND 239  : REM Blank screen
```

**Restore sequence:**

```basic
REM Restore display
POKE 53265, PEEK(53265) OR 16  : REM Turn screen back on
POKE 53280, 14  : REM Restore border color (example: light blue)
```

---

## VIC-II Control Registers

**Base Address:** 53248 ($D000)
**Range:** 53248-53294 ($D000-$D02E) - 47 registers total

### Key Graphics Registers

| Register | Decimal | Hex | Function |
|----------|---------|-----|----------|
| 53265 | $D011 | Extended background (bit 6), bitmap (bit 5), screen blank (bit 4) |
| 53270 | $D016 | Horizontal scroll, multicolor mode (bit 4) |
| 53272 | $D018 | Screen memory (bits 4-7), char memory (bits 1-3) |
| 53280 | $D020 | Border color |
| 53281 | $D021 | Background color #0 (screen color) |
| 53282 | $D022 | Background color #1 (multicolor/extended) |
| 53283 | $D023 | Background color #2 (multicolor/extended) |
| 53284 | $D024 | Background color #3 (extended mode only) |

---

## Programmable Characters - Memory Protection

When creating custom character sets in RAM for BASIC programs, you must protect that memory from BASIC.

### Protection Method

```basic
REM Reserve top of memory for character set
POKE 52, 48 : POKE 56, 48 : CLR
```

This moves BASIC's top-of-memory pointer down, protecting memory from 12288 ($3000) upward.

**Why location 12288?**
- It's a valid character memory start address (A=12 in table above)
- It's at a 2K boundary (required for character sets)
- It leaves 12K for BASIC programs (still plenty)

### Starting Addresses for Character Sets (with BASIC)

**Valid locations:** Any 2K boundary within the VIC-II's 16K bank

**Recommended for BASIC:** 12288 ($3000)
- Below this: Available for BASIC program
- At this location: 2K character set
- Above this: More free RAM

**Do NOT use with BASIC:**
- 0 ($0000) - System zero page
- 2048 ($0800) - BASIC program start

---

## Important Notes

### Chroma Noise Prevention

Always make vertical lines at least **2 bits wide** in character definitions to prevent color distortion on TV screens.

### Reversed Characters

Character codes 128-255 display the same patterns as 0-127 but in **reverse video** (foreground/background colors swapped).

Reversal formula: `REVERSED_CODE = NORMAL_CODE + 128`

### Screen Code vs PETSCII

- **Screen codes** (0-255): Used with POKE to screen memory
- **PETSCII codes**: Used with PRINT and CHR$()
- These are **different** - don't confuse them!

Example: Letter "A"
- Screen code: 1
- PETSCII code: 65

---

## Common Patterns

### Copy ROM Characters to RAM

```basic
10 POKE 56334, PEEK(56334) AND 254 : REM Interrupts off
20 POKE 1, PEEK(1) AND 251 : REM Switch in char ROM
30 FOR I = 0 TO 511
40 POKE 12288 + I, PEEK(53248 + I)
50 NEXT I
60 POKE 1, PEEK(1) OR 4 : REM Switch in I/O
70 POKE 56334, PEEK(56334) OR 1 : REM Interrupts on
```

### Activate Custom Character Set

```basic
POKE 53272, (PEEK(53272) AND 240) + 12
```

This points character memory to location 12288 within the current bank.

### Design Characters Using Binary

```basic
10 FOR I = 12448 TO 12455 : REM Character 20 (@12288 + 20*8)
20 READ A : POKE I, A
30 NEXT I
40 DATA 60, 66, 165, 129, 165, 153, 66, 60
```

Each DATA value is one row. Calculate by adding bit values (128,64,32,16,8,4,2,1) for each 1 bit in the pattern.

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for memory maps
- **Related:** See SCREEN-CODES-REFERENCE.md for character code tables

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications
