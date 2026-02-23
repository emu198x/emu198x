# 6566/6567 VIC-II Video Interface Chip Reference

**⚠️ FOR LESSON CREATION: See [VIC-II-QUICK-REFERENCE.md](VIC-II-QUICK-REFERENCE.md) first (80% smaller, programming-focused)**

**Document Version:** 1.0
**Status:** Complete (All 4 parts processed)
**Last Updated:** 2025-10-18
**Target Audience:** Assembly language programmers, graphics programmers, game developers

---

## Quick Start

**If you're creating lessons**, you probably want [VIC-II-QUICK-REFERENCE.md](VIC-II-QUICK-REFERENCE.md) instead. It contains:
- Sprite setup and movement patterns
- Color and register tables
- Raster interrupt examples
- Common graphics tasks
- **17KB vs 87KB** (this file)

**This comprehensive reference** contains hardware specifications useful for deep technical work:
- Pin configurations and electrical characteristics
- Timing specifications and sync signals
- Hardware architecture details
- Advanced display mode internals

---

## Document Purpose

This reference provides comprehensive technical documentation for the 6566/6567 VIC-II (Video Interface Chip II) used in the Commodore 64. It synthesizes information from the C64 Programmer's Reference Guide with practical programming guidance and C64-specific examples.

**What this document covers:**
- Character display modes (standard, multicolor, extended color)
- Bitmap graphics modes (standard and multicolor)
- Movable Object Blocks (MOBs/sprites)
- Display modes and memory organization
- Color control and collision detection
- Register-level programming

**Curriculum mapping:**
- **Tier 2 (Hardware Fundamentals)**: Basic screen memory, color RAM, character sets
- **Tier 3 (Assembly Mastery)**: VIC-II register programming, raster effects
- **Tier 4 (Advanced Techniques)**: Sprite multiplexing, FLD, flexible line distance

---

## Table of Contents

1. [Overview](#overview)
2. [Character Display Modes](#character-display-modes)
3. [Bitmap Display Modes](#bitmap-display-modes)
4. [Movable Object Blocks (Sprites)](#movable-object-blocks-sprites)
5. [Other Features](#other-features)
6. [Programming Examples](#programming-examples)
7. [Quick Reference Tables](#quick-reference-tables)

---

## Overview

### What is the VIC-II?

The 6566/6567 VIC-II is a multi-purpose color video controller chip designed for computer video terminals and video game applications. It's the heart of the Commodore 64's graphics capabilities.

**Key Features:**
- **47 control registers** accessible via memory-mapped I/O ($D000-$D02E)
- **16K address space** for display information
- **320×200 maximum resolution** in bitmap mode
- **8 hardware sprites** (Movable Object Blocks)
- **16 colors** available simultaneously
- **Multiple display modes** for flexibility
- **Hardware collision detection** for games
- **Smooth scrolling** in both axes

**VIC-II Chip Variants:**

| Chip | Video System | Timing | Used In |
|------|--------------|--------|---------|
| **6567** | NTSC | 60 Hz, 525 lines | North American C64 |
| **6566** | PAL | 50 Hz, 625 lines | European C64 |

**Memory Architecture:**

The VIC-II can address 16K of memory at a time (14 address lines: A0-A13). The VIC-II's view of memory is controlled by:
1. **CIA #2 Port A** (bits 0-1): Selects which 16K bank ($0000, $4000, $8000, $C000)
2. **VIC-II registers**: Control addressing within the 16K bank

```assembly
; Select VIC bank (via CIA #2)
LDA $DD02       ; Data direction register
ORA #%00000011  ; Set bits 0-1 as outputs
STA $DD02

LDA $DD00       ; Port A
AND #%11111100  ; Clear bits 0-1
ORA #%00000010  ; Select bank (bits are INVERTED)
STA $DD00       ; %11 = bank 0, %10 = bank 1, %01 = bank 2, %00 = bank 3
```

**Base Addresses (on C64):**

| Register/Location | C64 Address | Size | Purpose |
|-------------------|-------------|------|---------|
| VIC-II Registers | $D000-$D02E | 47 bytes | Control registers |
| Screen Memory (default) | $0400-$07E7 | 1000 bytes | Video matrix |
| Character ROM (default) | $1000-$1FFF | 2048 bytes | Character definitions |
| Color RAM | $D800-$DBE7 | 1000 nybbles | Character color data |
| Sprite Pointers | Last 8 bytes of screen | 8 bytes | MOB data pointers |

---

## Character Display Modes

Character mode is the foundation of C64 text display and many games. The VIC-II fetches character pointers from the **VIDEO MATRIX** and translates them to dot patterns from the **CHARACTER BASE**.

### Memory Organization

**VIDEO MATRIX:**
- **Size:** 1000 consecutive bytes (25 rows × 40 columns)
- **Location:** Defined by VM13-VM10 in register $18
- **Content:** 8-bit character pointers (0-255)
- **Associated data:** 4-bit color nybble per character (12-bit wide memory)

**CHARACTER BASE:**
- **Size:** 2048 bytes (256 characters × 8 bytes each)
- **Location:** Defined by CB13-CB11 in register $18
- **Format:** Each character is 8×8 dot matrix stored as 8 consecutive bytes

### Addressing Architecture

**Character Pointer Address:**

```
A13 A12 A11 A10 A09 A08 A07 A06 A05 A04 A03 A02 A01 A00
VM13 VM12 VM11 VM10 VC9 VC8 VC7 VC6 VC5 VC4 VC3 VC2 VC1 VC0
```

- **VM13-VM10:** Video matrix base (from register $18, bits 7-4)
- **VC9-VC0:** Internal video counter (steps through 1000 locations)

**Character Data Address:**

```
A13 A12 A11 A10 A09 A08 A07 A06 A05 A04 A03 A02 A01 A00
CB13 CB12 CB11 D7  D6  D5  D4  D3  D2  D1  D0  RC2 RC1 RC0
```

- **CB13-CB11:** Character base (from register $18, bits 3-1)
- **D7-D0:** Character pointer from video matrix (selects character 0-255)
- **RC2-RC0:** Raster counter (selects row 0-7 within character)

**C64-Specific Example:**

```assembly
; Configure screen at $0400, character set at $2000

LDA $D018       ; Memory setup register
AND #%00001111  ; Clear screen memory bits
ORA #%00010000  ; VM = %0001 = $0400 (screen at $0400-$07E7)
STA $D018

LDA $D018
AND #%11110001  ; Clear character base bits
ORA #%00000100  ; CB = %010 = $2000 (characters at $2000-$27FF)
STA $D018
```

### Standard Character Mode

**Mode bits:** MCM = 0, BMM = 0, ECM = 0

Standard character mode provides 256 characters with individual colors and a shared background color.

**Color interpretation:**

| Character Bit | Function | Color Displayed |
|---------------|----------|-----------------|
| 0 | Background | Background #0 (register $D021) |
| 1 | Foreground | Color from 4-bit color nybble (Color RAM) |

**Resolution:** 8×8 dots per character
**Characters available:** 256
**Colors per character:** 2 (1 foreground + 1 shared background)
**Foreground colors available:** 16 (from color nybble)

**Example:**

```assembly
; Display "HELLO" in white on blue

; Set background color
LDA #$06        ; Blue
STA $D021       ; Background #0

; Set screen memory
LDX #$00
LOOP:
    LDA MESSAGE,X
    STA $0400,X     ; Character to screen
    LDA #$01        ; White
    STA $D800,X     ; Color to color RAM
    INX
    CPX #$05        ; 5 characters
    BNE LOOP
    RTS

MESSAGE:
    .BYTE $08,$05,$0C,$0C,$0F  ; "HELLO" in PETSCII/screen codes
```

**Character format in memory:**

```
Character 'A' definition (8 bytes):
Byte 0: %00011000  = $18    ···XX···
Byte 1: %00111100  = $3C    ··XXXX··
Byte 2: %01100110  = $66    ·XX··XX·
Byte 3: %01111110  = $7E    ·XXXXXX·
Byte 4: %01100110  = $66    ·XX··XX·
Byte 5: %01100110  = $66    ·XX··XX·
Byte 6: %01100110  = $66    ·XX··XX·
Byte 7: %00000000  = $00    ········
```

### Multicolor Character Mode

**Mode bits:** MCM = 1, BMM = 0, ECM = 0

Multicolor mode allows up to 4 colors per character but with reduced horizontal resolution (4×8 instead of 8×8).

**Enabling multicolor mode:**

```assembly
LDA $D016       ; Control Register 2
ORA #%00010000  ; Set MCM bit (bit 4)
STA $D016
```

**Per-character mode selection:**

Each character can be individually selected for multicolor or standard mode using the MSB of the color nybble:

- **Color nybble MSB = 0:** Display as standard mode (8×8, colors 0-7 only)
- **Color nybble MSB = 1:** Display as multicolor mode (4×8, 4 colors)

**Color interpretation (when MCM:MSB(CM) = 1):**

| Bit Pair | Function | Color Displayed |
|----------|----------|-----------------|
| 00 | Background | Background #0 (register $D021) |
| 01 | Background | Background #1 (register $D022) |
| 10 | Foreground | Background #2 (register $D023) |
| 11 | Foreground | Lower 3 bits of color nybble (colors 0-7) |

**Resolution:** 4×8 (each dot is 2 pixels wide)
**Colors per character:** 4
**Foreground colors available:** 8 (colors 0-7 from lower 3 bits)

**Example:**

```assembly
; Create multicolor character display

; Set multicolor mode
LDA $D016
ORA #%00010000  ; MCM = 1
STA $D016

; Set shared background colors
LDA #$00        ; Black
STA $D021       ; Background #0
LDA #$0B        ; Dark gray
STA $D022       ; Background #1
LDA #$0C        ; Medium gray
STA $D023       ; Background #2

; Display multicolor character
LDA #$01        ; Character 1
STA $0400       ; To screen
LDA #$0F        ; White + multicolor bit ($08 + $07 = $0F)
STA $D800       ; Color RAM (MSB=1 enables multicolor for this char)
```

**Multicolor character format:**

```
Multicolor character (8 bytes, each bit pair = 1 wide pixel):
Byte 0: %11001001  =  XX··X··X  (4 pixels wide)
Byte 1: %11110011  =  XXXX··XX
Byte 2: %10011100  =  X··XXX··
...

Bit pair interpretation:
  %00 = Background #0 (register $D021)
  %01 = Background #1 (register $D022)
  %10 = Background #2 (register $D023)
  %11 = Character color from lower 3 bits of color RAM
```

**Mixing modes on same screen:**

```assembly
; Character at $0400: Standard mode (high-res)
LDA #$41        ; Character 'A'
STA $0400
LDA #$01        ; White (bit 3 = 0, standard mode)
STA $D800

; Character at $0401: Multicolor mode
LDA #$42        ; Character 'B'
STA $0401
LDA #$09        ; Red + multicolor (bit 3 = 1, multicolor mode)
STA $D801
```

### Extended Color Mode

**Mode bits:** ECM = 1, BMM = 0, MCM = 0

Extended color mode provides individual background colors for each character while maintaining 8×8 resolution.

**WARNING:** Extended color mode and multicolor mode should NOT be enabled simultaneously!

**Enabling extended color mode:**

```assembly
LDA $D011       ; Control Register 1
ORA #%01000000  ; Set ECM bit (bit 6)
STA $D011
```

**Character pointer interpretation:**

The 2 MSB of the character pointer select the background color:

| Bits 7-6 | Background Color |
|----------|------------------|
| 00 | Background #0 (register $D021) |
| 01 | Background #1 (register $D022) |
| 10 | Background #2 (register $D023) |
| 11 | Background #3 (register $D024) |

**Character selection:**

Since bits 7-6 are used for color, only the lower 6 bits (bits 5-0) select the character:
- **Only 64 characters available** (0-63)
- VIC-II forces CB10 and CB9 to 0, using only first 64 character definitions
- Character pointers $00-$3F, $40-$7F, $80-$BF, $C0-$FF all access chars 0-63

**Color interpretation:**

| Character Bit | Function | Color Displayed |
|---------------|----------|-----------------|
| 0 | Background | One of 4 backgrounds (selected by char pointer bits 7-6) |
| 1 | Foreground | Color from 4-bit color nybble (Color RAM) |

**Resolution:** 8×8 dots per character
**Characters available:** 64
**Colors per character:** 2 (1 foreground + 1 of 4 backgrounds)

**Example:**

```assembly
; Extended color mode with colored backgrounds

; Enable extended color mode
LDA $D011
ORA #%01000000  ; ECM = 1
STA $D011

; Set 4 background colors
LDA #$00        ; Black
STA $D021       ; Background #0
LDA #$02        ; Red
STA $D022       ; Background #1
LDA #$05        ; Green
STA $D023       ; Background #2
LDA #$0E        ; Light blue
STA $D024       ; Background #3

; Display characters with different backgrounds
LDA #%00000001  ; Character 1, background #0 (bits 7-6 = %00)
STA $0400
LDA #%01000001  ; Character 1, background #1 (bits 7-6 = %01)
STA $0401
LDA #%10000001  ; Character 1, background #2 (bits 7-6 = %10)
STA $0402
LDA #%11000001  ; Character 1, background #3 (bits 7-6 = %11)
STA $0403

; All use same foreground color from color RAM
LDA #$01        ; White foreground
STA $D800
STA $D801
STA $D802
STA $D803
```

**Common use case: Text with colored backgrounds**

```assembly
; Create colored menu items

MENU_ITEM_1:
    LDA #%00100000  ; Space character, background #0 (black)
    STA $0400

MENU_ITEM_2:
    LDA #%01100000  ; Space character, background #1 (red = selected)
    STA $0428       ; Next row

MENU_ITEM_3:
    LDA #%00100000  ; Space character, background #0 (black)
    STA $0450       ; Next row
```

---

## Bitmap Display Modes

Bitmap mode provides direct pixel control with a one-to-one correspondence between each displayed dot and a memory bit, enabling high-resolution graphics.

### Bitmap Memory Organization

**Display resolution:** 320 horizontal × 200 vertical pixels

**Memory requirements:**
- **Bitmap data:** 8000 bytes (320 × 200 ÷ 8 bits/byte)
- **Color data:** 1000 bytes from video matrix (used differently than character mode)

**Enabling bitmap mode:**

```assembly
LDA $D011       ; Control Register 1
ORA #%00100000  ; Set BMM bit (bit 5)
STA $D011
```

### Bitmap Addressing

**Display Base Address:**

```
A13 A12 A11 A10 A09 A08 A07 A06 A05 A04 A03 A02 A01 A00
CB13 VC9 VC8 VC7 VC6 VC5 VC4 VC3 VC2 VC1 VC0 RC2 RC1 RC0
```

- **CB13:** Bitmap base address (from register $18, bit 3)
  - CB13 = 0: Bitmap at $0000-$1F3F
  - CB13 = 1: Bitmap at $2000-$3F3F
- **VC9-VC0:** Video matrix counter (steps through 40 locations/row, 25 rows)
- **RC2-RC0:** Raster counter (8 rows per block)

**Key difference from character mode:**

The video matrix counter now addresses bitmap data directly, not character pointers. Each 8 sequential bytes form an 8×8 pixel block on screen.

**Bitmap organization:**

```
Screen layout (40×25 character blocks):

Block 0,0   Block 1,0   Block 2,0   ...  Block 39,0
Block 0,1   Block 1,1   Block 2,1   ...  Block 39,1
...
Block 0,24  Block 1,24  Block 2,24  ...  Block 39,24

Each block = 8 bytes in bitmap memory:
Block 0,0: Bytes 0-7   (screen position $0400)
Block 1,0: Bytes 8-15  (screen position $0401)
Block 2,0: Bytes 16-23 (screen position $0402)
...
```

**Memory formula:**

For pixel at screen position (X, Y):
- **Block number** = (Y ÷ 8) × 40 + (X ÷ 8)
- **Byte offset** = Block × 8 + (Y mod 8)
- **Bit position** = 7 - (X mod 8)

**Example:**

```assembly
; Calculate bitmap byte for pixel at (100, 50)
; X=100, Y=50

; Block = (50 ÷ 8) × 40 + (100 ÷ 8)
;       = 6 × 40 + 12
;       = 240 + 12 = 252

; Byte offset = 252 × 8 + (50 mod 8)
;             = 2016 + 2 = 2018

; Bit position = 7 - (100 mod 8)
;              = 7 - 4 = 3

LDA BITMAP+2018 ; Read byte containing pixel
ORA #%00001000  ; Set bit 3 to turn pixel on
STA BITMAP+2018
```

### Standard Bitmap Mode

**Mode bits:** BMM = 1, MCM = 0

In standard bitmap mode, color information comes from the video matrix (not color RAM). Each 8×8 block can have 2 colors.

**Color data from video matrix:**

Each byte in the video matrix (normally used for character pointers) is divided into two 4-bit nybbles:
- **Upper nybble (bits 7-4):** Color for "1" bits in bitmap
- **Lower nybble (bits 3-0):** Color for "0" bits in bitmap

**Color interpretation:**

| Bitmap Bit | Color Source |
|------------|--------------|
| 0 | Lower nybble of video matrix |
| 1 | Upper nybble of video matrix |

**Resolution:** 320×200 pixels
**Colors per 8×8 block:** 2

**Example:**

```assembly
; Set up standard bitmap mode at $2000

LDA $D018
AND #%11110111  ; Clear CB13
ORA #%00001000  ; Set CB13 = 1 (bitmap at $2000)
STA $D018

LDA $D011
ORA #%00100000  ; Set BMM bit
STA $D011

; Set colors for block 0 (top-left corner)
LDA #$1E        ; Upper nybble = $1 (white), lower = $E (light blue)
STA $0400       ; Video matrix for block 0

; Draw pattern in block 0
LDA #%11110000
STA $2000       ; Row 0: ████····
LDA #%11001100
STA $2001       ; Row 1: ██··██··
LDA #%10101010
STA $2002       ; Row 2: █·█·█·█·
; ... continue for 8 rows
```

**Screen clear routine:**

```assembly
; Clear bitmap to background color

CLEAR_BITMAP:
    LDA #$00        ; All pixels off
    LDX #$00
LOOP1:
    STA $2000,X     ; Clear pages $20-$3F
    STA $2100,X
    STA $2200,X
    ; ... continue for all pages
    STA $3F00,X
    INX
    BNE LOOP1
    RTS
```

### Multicolor Bitmap Mode

**Mode bits:** BMM = 1, MCM = 1

Multicolor bitmap mode allows 4 colors per 8×8 block but with half the horizontal resolution (160×200).

**Enabling multicolor bitmap mode:**

```assembly
LDA $D011
ORA #%00100000  ; BMM = 1
STA $D011

LDA $D016
ORA #%00010000  ; MCM = 1
STA $D016
```

**Color interpretation:**

| Bit Pair | Color Source |
|----------|--------------|
| 00 | Background #0 (register $D021) |
| 01 | Upper nybble of video matrix |
| 10 | Lower nybble of video matrix |
| 11 | Color nybble (Color RAM at $D800) |

**Resolution:** 160×200 (each pixel is 2 bits wide)
**Colors per 8×8 block:** 4 (1 global background + 3 individual)

**Example:**

```assembly
; Set up multicolor bitmap mode

LDA $D011
ORA #%00100000  ; BMM = 1
STA $D011

LDA $D016
ORA #%00010000  ; MCM = 1
STA $D016

; Set global background
LDA #$00        ; Black
STA $D021

; Set colors for block 0
LDA #$CE        ; Upper=$C (red), lower=$E (light blue)
STA $0400       ; Video matrix
LDA #$0F        ; White (from color RAM)
STA $D800

; Draw multicolor pattern in block 0
; Each bit pair = 1 wide pixel
LDA #%11100100  ; Colors: 11 10 01 00 = white, lt.blue, red, black
STA $2000       ; Row 0
```

**Pixel plotting routine:**

```assembly
; Plot multicolor pixel at (X=50, Y=100)
; Color in .A (0-3)

PLOT_MC_PIXEL:
    ; Calculate block: (100÷8)×40 + (50÷8) = 12×40 + 6 = 486
    ; Byte offset: 486×8 + (100 mod 8) = 3888 + 4 = 3892
    ; Bit pair position: (7 - (50 mod 2×2)) = bits 5-4

    ; This is complex - typically use lookup tables!
    ; See programming examples section for optimized routines
```

---

## Movable Object Blocks (Sprites)

Movable Object Blocks (MOBs), commonly called **sprites**, are special graphics objects that can be positioned independently of the character/bitmap display. The VIC-II supports 8 simultaneous sprites.

### Sprite Fundamentals

**Sprite specifications:**
- **Count:** 8 sprites (MOB 0 - MOB 7)
- **Size:** 24×21 pixels (standard), 48×42 (2× expanded)
- **Memory:** 63 bytes per sprite + 1 padding byte
- **Position:** 512 horizontal × 256 vertical positions
- **Colors:** Individual color per sprite + shared multicolor

**Sprite data format:**

Each sprite is defined by 63 bytes arranged as 21 rows × 3 bytes:

```
Sprite memory layout (63 bytes):

Byte 0   Byte 1   Byte 2      ← Row 0
Byte 3   Byte 4   Byte 5      ← Row 1
Byte 6   Byte 7   Byte 8      ← Row 2
...
Byte 57  Byte 58  Byte 59     ← Row 19
Byte 60  Byte 61  Byte 62     ← Row 20

Each row = 24 bits (3 bytes) = 24 pixels wide
Total = 21 rows × 3 bytes = 63 bytes
(Actual sprite block = 64 bytes, last byte ignored)
```

**Example sprite data (arrow pointing right):**

```assembly
SPRITE_DATA:
    .BYTE %00000001, %10000000, %00000000  ; Row 0:    ·······XX·······
    .BYTE %00000011, %11000000, %00000000  ; Row 1:    ······XXXX······
    .BYTE %00000111, %11100000, %00000000  ; Row 2:    ·····XXXXXX·····
    .BYTE %00001111, %11110000, %00000000  ; Row 3:    ····XXXXXXXX····
    .BYTE %00011111, %11111000, %00000000  ; Row 4:    ···XXXXXXXXXX···
    .BYTE %00111111, %11111100, %00000000  ; Row 5:    ··XXXXXXXXXXXX··
    .BYTE %01111111, %11111110, %00000000  ; Row 6:    ·XXXXXXXXXXXXXX·
    .BYTE %11111111, %11111111, %00000000  ; Row 7:    XXXXXXXXXXXXXXXX
    ; ... continue for 21 rows
```

### Enabling and Positioning Sprites

**Enable register:** $D015 (21 decimal)

Each bit enables the corresponding sprite:

```assembly
; Enable sprites 0, 1, and 2
LDA #%00000111  ; Bits 0-2 set
STA $D015       ; Sprite enable register

; Disable all sprites
LDA #%00000000
STA $D015
```

**Position registers:**

Each sprite has separate X and Y position registers:

| Sprite | X Position | Y Position |
|--------|------------|------------|
| 0 | $D000 | $D001 |
| 1 | $D002 | $D003 |
| 2 | $D004 | $D005 |
| 3 | $D006 | $D007 |
| 4 | $D008 | $D009 |
| 5 | $D00A | $D00B |
| 6 | $D00C | $D00D |
| 7 | $D00E | $D00F |

**X MSB register:** $D010 (16 decimal)

The X position is 9 bits (0-511) to cover the full horizontal range. Bit 8 is stored in register $D010:

```assembly
; Position sprite 0 at X=400, Y=100

; Set Y position (8 bits, simple)
LDA #100
STA $D001       ; Sprite 0 Y position

; Set X position (9 bits, requires MSB register)
LDA #144        ; Lower 8 bits of 400 (%1 10010000)
STA $D000       ; Sprite 0 X position

LDA $D010       ; X MSB register
ORA #%00000001  ; Set bit 0 (sprite 0 MSB)
STA $D010       ; X position now = 256 + 144 = 400
```

**Visible screen area:**

- **X positions 23-347** ($17-$15B): Fully or partially visible
- **Y positions 50-249** ($32-$F9): Fully or partially visible

Sprites can be smoothly moved on/off screen edges.

**Example: Move sprite across screen**

```assembly
; Animate sprite 0 from left to right

MOVE_SPRITE:
    LDX SPRITE_X_LO ; Get current X position (low byte)
    INX             ; Increment
    STX SPRITE_X_LO
    STX $D000       ; Update VIC-II

    ; Check for carry to bit 8
    BNE NO_CARRY

    ; Increment high bit
    LDA $D010
    ORA #%00000001  ; Set sprite 0 MSB
    STA $D010

NO_CARRY:
    ; Check if off right edge
    CPX #$5B        ; X > 347?
    BCC CONTINUE
    LDA $D010
    AND #%00000001
    BNE WRAPAROUND  ; MSB=1 and X>347 = off screen

CONTINUE:
    RTS

WRAPAROUND:
    ; Reset to left edge
    LDA #23
    STA $D000
    LDA $D010
    AND #%11111110  ; Clear sprite 0 MSB
    STA $D010
    RTS

SPRITE_X_LO: .BYTE 23
```

### Sprite Memory Pointers

Sprite data pointers are stored in the last 8 bytes of the video matrix area.

**Pointer locations:**

For default screen at $0400:
- Sprite 0 pointer: $0400 + $3F8 = $07F8
- Sprite 1 pointer: $0400 + $3F9 = $07F9
- Sprite 2 pointer: $0400 + $3FA = $07FA
- Sprite 3 pointer: $0400 + $3FB = $07FB
- Sprite 4 pointer: $0400 + $3FC = $07FC
- Sprite 5 pointer: $0400 + $3FD = $07FD
- Sprite 6 pointer: $0400 + $3FE = $07FE
- Sprite 7 pointer: $0400 + $3FF = $07FF

**Pointer interpretation:**

The 8-bit pointer × 64 = sprite data address (within current VIC bank).

**Sprite Data Address:**

```
A13 A12 A11 A10 A09 A08 A07 A06 A05 A04 A03 A02 A01 A00
MP7 MP6 MP5 MP4 MP3 MP2 MP1 MP0 MC5 MC4 MC3 MC2 MC1 MC0
```

- **MP7-MP0:** Sprite pointer from video matrix ($07F8-$07FF)
- **MC5-MC0:** Internal sprite byte counter (0-63)

**Example:**

```assembly
; Place sprite data at $2000 (within VIC bank)
; Sprite pointer = $2000 ÷ 64 = $80 (128 decimal)

; Copy sprite data to $2000
LDX #63
COPY_LOOP:
    LDA SPRITE_DATA,X
    STA $2000,X
    DEX
    BPL COPY_LOOP

; Set sprite 0 pointer
LDA #$80        ; Pointer = $2000 ÷ 64
STA $07F8       ; Sprite 0 pointer

; Enable and position sprite 0
LDA #%00000001
STA $D015       ; Enable sprite 0

LDA #100
STA $D000       ; X position
LDA #100
STA $D001       ; Y position

; Set sprite color
LDA #$01        ; White
STA $D027       ; Sprite 0 color
```

**Pointer update timing:**

Sprite pointers are read from video matrix at the **end of every raster line**. When Y position matches current raster line, sprite data fetching begins.

### Sprite Colors

**Standard sprite mode:** Each sprite has individual color register

**Color registers:**

| Sprite | Color Register | Address |
|--------|----------------|---------|
| 0 | Sprite 0 Color | $D027 |
| 1 | Sprite 1 Color | $D028 |
| 2 | Sprite 2 Color | $D029 |
| 3 | Sprite 3 Color | $D02A |
| 4 | Sprite 4 Color | $D02B |
| 5 | Sprite 5 Color | $D02C |
| 6 | Sprite 6 Color | $D02D |
| 7 | Sprite 7 Color | $D02E |

**Standard sprite color interpretation (MnMC = 0):**

| Sprite Data Bit | Color Displayed |
|-----------------|-----------------|
| 0 | Transparent (background shows through) |
| 1 | Sprite color from register $D027-$D02E |

**Example:**

```assembly
; Set sprite colors
LDA #$01        ; White
STA $D027       ; Sprite 0
LDA #$02        ; Red
STA $D028       ; Sprite 1
LDA #$05        ; Green
STA $D029       ; Sprite 2
```

### Multicolor Sprites

**Multicolor control:** Register $D01C (28 decimal)

Each bit enables multicolor mode for corresponding sprite:

```assembly
; Enable multicolor mode for sprites 0 and 1
LDA #%00000011  ; Bits 0-1 set
STA $D01C       ; Multicolor sprite register
```

**Shared multicolor registers:**
- **Multicolor #0:** $D025 (37 decimal) - shared by all multicolor sprites
- **Multicolor #1:** $D026 (38 decimal) - shared by all multicolor sprites

**Multicolor sprite interpretation (MnMC = 1):**

| Bit Pair | Color Displayed |
|----------|-----------------|
| 00 | Transparent |
| 01 | Sprite Multicolor #0 (register $D025) |
| 10 | Sprite individual color (register $D027-$D02E) |
| 11 | Sprite Multicolor #1 (register $D026) |

**Resolution:** 12×21 (each pixel is 2 bits wide)
**Colors per sprite:** 3 + transparent

**Example:**

```assembly
; Set up multicolor sprites

; Enable multicolor mode for sprite 0
LDA #%00000001
STA $D01C

; Set shared multicolor registers
LDA #$0B        ; Dark gray
STA $D025       ; Multicolor #0 (bit pair %01)
LDA #$0C        ; Medium gray
STA $D026       ; Multicolor #1 (bit pair %11)

; Set sprite 0 individual color
LDA #$0F        ; Light gray
STA $D027       ; Sprite 0 color (bit pair %10)

; Sprite data interpretation:
; %00 = Transparent
; %01 = Dark gray ($D025)
; %10 = Light gray ($D027)
; %11 = Medium gray ($D026)
```

**Multicolor sprite data example:**

```assembly
MULTICOLOR_SPRITE:
    ; Each bit pair = 1 wide pixel
    .BYTE %11111111, %11111111, %00000000  ; Row 0: ████████ (all %11)
    .BYTE %11111001, %10011111, %00000000  ; Row 1: ████··██
    .BYTE %11110000, %00001111, %00000000  ; Row 2: ████····████
    ; ... 21 rows total
```

### Sprite Expansion

Sprites can be expanded 2× in horizontal and/or vertical directions independently.

**Expansion registers:**
- **X expansion:** $D017 (23 decimal) - doubles width to 48 pixels
- **Y expansion:** $D01D (29 decimal) - doubles height to 42 pixels

Each bit controls the corresponding sprite:

```assembly
; Expand sprite 0 horizontally
LDA $D017
ORA #%00000001  ; Set bit 0
STA $D017       ; Sprite 0 now 48 pixels wide

; Expand sprite 0 vertically
LDA $D01D
ORA #%00000001  ; Set bit 0
STA $D01D       ; Sprite 0 now 42 pixels tall

; Sprite 0 is now 48×42 pixels (4× original area)
```

**Expansion behavior:**

No increase in resolution - the same 24×21 pattern is displayed with each pixel doubled in size.

**Expansion combinations:**

| X Expand | Y Expand | Size | Pixel Size |
|----------|----------|------|------------|
| 0 | 0 | 24×21 | 1×1 (normal) |
| 1 | 0 | 48×21 | 2×1 (wide) |
| 0 | 1 | 24×42 | 1×2 (tall) |
| 1 | 1 | 48×42 | 2×2 (double) |

**With multicolor mode:**

Multicolor + expansion can create very large pixels:

| Mode | X Expand | Effective Pixel Size |
|------|----------|---------------------|
| Standard | 0 | 1×1 |
| Standard | 1 | 2×1 |
| Multicolor | 0 | 2×1 |
| Multicolor | 1 | 4×1 (4× standard width!) |

### Sprite Priority

**Priority register:** $D01B (27 decimal)

Each bit controls whether the corresponding sprite appears in front of or behind display data.

**Priority bit interpretation:**

| Bit Value | Priority |
|-----------|----------|
| 0 | Sprite in front (non-transparent sprite data always displayed) |
| 1 | Sprite behind (sprite only visible over background) |

**Priority rules:**

When MnDP = 1 (sprite behind):
- Sprite displays over Background #0
- Sprite displays over multicolor bit pair %01
- Foreground data obscures sprite

When MnDP = 0 (sprite in front):
- Non-transparent sprite data always displayed over character/bitmap data

**Example:**

```assembly
; Set sprite 0 behind display data (for "under" effect)
LDA $D01B
ORA #%00000001  ; Set bit 0
STA $D01B

; Set sprite 1 in front (default game behavior)
LDA $D01B
AND #%11111101  ; Clear bit 1
STA $D01B
```

**Sprite-to-sprite priority:**

Sprites have fixed priority relative to each other:
- **Sprite 0:** Highest priority (always on top of other sprites)
- **Sprite 7:** Lowest priority (always behind other sprites)

When non-transparent data from two sprites overlaps, lower-numbered sprite is displayed.

**Priority resolution order:**

1. Sprite-to-sprite priority resolved first (lower number wins)
2. Then sprite-to-display priority applied (MnDP bit)

**Example: Creating parallax effect**

```assembly
; Background layer: Sprite 7 (behind display data)
LDA $D01B
ORA #%10000000  ; Sprite 7 behind
STA $D01B

; Foreground player: Sprite 0 (in front of display data)
LDA $D01B
AND #%11111110  ; Sprite 0 in front
STA $D01B

; Result: Player sprite over text over background sprite
```

### Collision Detection

The VIC-II provides hardware collision detection between sprites and between sprites and display data.

**Collision registers:**
- **Sprite-Sprite:** $D01E (30 decimal) - MOB to MOB collision
- **Sprite-Data:** $D01F (31 decimal) - MOB to display data collision

**Both registers auto-clear when read!**

#### Sprite-to-Sprite Collision

**Register $D01E:** Each bit represents a sprite that has collided with another sprite.

**Collision condition:**
- Non-transparent data from two sprites overlap
- Transparent areas (%00 in multicolor mode, 0 in standard mode) do NOT cause collision

**Behavior:**
- When sprites n and m collide, bits n AND m are both set
- Bits remain set until register is read
- Reading register clears ALL bits to 0
- Collisions detected even if sprites are off-screen

**Example:**

```assembly
; Check for sprite collisions

CHECK_COLLISIONS:
    LDA $D01E       ; Read collision register (clears it!)
    STA TEMP        ; MUST save immediately!

    ; Check if sprite 0 collided
    AND #%00000001
    BEQ NO_SPRITE0_COLLISION

    ; Sprite 0 hit something - check what
    LDA TEMP        ; Reload saved value
    AND #%11111110  ; Mask off sprite 0 bit
    ; Remaining bits show which sprite(s) it hit

NO_SPRITE0_COLLISION:
    RTS

TEMP: .BYTE 0
```

**CRITICAL:** Reading $D01E clears ALL flags. Always save the value immediately:

```assembly
; WRONG - second read returns $00
LDA $D01E       ; Read and clear
AND #%00000001  ; Check sprite 0
BEQ SKIP
LDA $D01E       ; THIS RETURNS $00 - already cleared!
AND #%00000010  ; Never works!

; RIGHT - save first read
LDA $D01E       ; Read and clear
STA TEMP        ; Save immediately
AND #%00000001  ; Check sprite 0
BEQ CHECK2
; Handle sprite 0 collision

CHECK2:
    LDA TEMP        ; Reload saved value
    AND #%00000010  ; Check sprite 1
    BEQ DONE
    ; Handle sprite 1 collision
DONE:
```

#### Sprite-to-Data Collision

**Register $D01F:** Each bit represents a sprite that has collided with foreground display data.

**Collision condition:**
- Sprite non-transparent data overlaps with:
  - Character/bitmap foreground data (bit = 1)
  - Extended color mode foreground
  - Multicolor %10 or %11 bit pairs
- Does NOT collide with:
  - Background #0
  - Multicolor %01 bit pair (special: allows background animation)
  - Transparent sprite areas

**Behavior:**
- Bit set when sprite and display data coincide
- Auto-clears when register is read
- Can detect collisions off-screen horizontally (if display data scrolled off)
- Cannot detect collisions off-screen vertically

**Example:**

```assembly
; Detect if player sprite hit wall

CHECK_WALL_HIT:
    LDA $D01F       ; Read sprite-data collision
    AND #%00000001  ; Check sprite 0 (player)
    BEQ NO_HIT

    ; Player hit wall - stop movement
    JSR STOP_PLAYER
    JSR PLAY_CRASH_SOUND

NO_HIT:
    RTS
```

**Multicolor %01 special case:**

Multicolor bit pair %01 does NOT cause sprite collisions. This allows animated backgrounds without interfering with gameplay:

```assembly
; Game screen with animated water
; Water uses multicolor %01 (Background #1)
; Player sprite can move over water without collision
; Walls use %10 or %11 - these DO cause collision

LDA #$06        ; Blue
STA $D022       ; Background #1 (%01) = water
LDA #$05        ; Green
STA $D023       ; Background #2 (%10) = grass (solid)
```

#### Collision Interrupt Latches

The VIC-II can generate interrupts when collisions occur.

**Interrupt behavior:**
- Interrupt latch set when **first bit** of collision register goes from 0→1
- Subsequent collisions do NOT set latch until register cleared
- Must read collision register to re-enable interrupt detection

**Example interrupt handler:**

```assembly
IRQ_HANDLER:
    ; Check if collision interrupt
    LDA $D019       ; VIC interrupt register
    AND #%00000100  ; Bit 2 = collision interrupt
    BEQ NOT_COLLISION

    ; Clear VIC interrupt
    LDA #%00000100
    STA $D019

    ; Read collision registers (clears them)
    LDA $D01E       ; Sprite-sprite
    STA SPRITE_COLLISIONS
    LDA $D01F       ; Sprite-data
    STA DATA_COLLISIONS

    ; Process collisions
    JSR HANDLE_COLLISIONS

NOT_COLLISION:
    JMP $EA31       ; Continue to KERNAL IRQ handler
```

---

## Other Features

### Screen Blanking

**Blank bit:** DEN in register $D011 (bit 4)

Setting DEN = 0 blanks the entire screen to the exterior/border color.

**Blanking register:** $D011

```assembly
; Blank screen
LDA $D011
AND #%11101111  ; Clear DEN bit (bit 4)
STA $D011       ; Screen now shows border color everywhere

; Unblank screen
LDA $D011
ORA #%00010000  ; Set DEN bit
STA $D011       ; Normal display
```

**Memory access during blanking:**

When DEN = 0:
- VIC-II only performs Phase 1 (transparent) memory accesses
- CPU has full system bus utilization
- MOB data still fetched if sprites enabled

**Border color:** Register $D020 (32 decimal)

```assembly
; Set border color
LDA #$00        ; Black
STA $D020       ; Border/exterior color
```

**Use cases:**
- Fast memory operations during vertical blank
- Screen transitions/fades
- Hiding display during mode changes

---

## Programming Examples

### Complete Sprite Setup

```assembly
;=============================================================================
; SPRITE SETUP EXAMPLE
; Sets up sprite 0 as animated spaceship
;=============================================================================

SPRITE_INIT:
    ; Copy sprite data to $2000
    LDX #63
COPY_LOOP:
    LDA SHIP_SPRITE,X
    STA $2000,X
    DEX
    BPL COPY_LOOP

    ; Set sprite pointer
    ; Sprite at $2000 = pointer $80 ($2000 / 64)
    LDA #$80
    STA $07F8       ; Sprite 0 pointer

    ; Enable sprite 0
    LDA #%00000001
    STA $D015

    ; Position sprite
    LDA #160        ; Center horizontally (approx)
    STA $D000       ; X position
    LDA #100        ; Upper area
    STA $D001       ; Y position

    ; Clear X MSB for sprite 0
    LDA $D010
    AND #%11111110
    STA $D010

    ; Set color
    LDA #$0E        ; Light blue
    STA $D027       ; Sprite 0 color

    ; Normal size, in front
    LDA $D017
    AND #%11111110  ; No X expansion
    STA $D017
    LDA $D01D
    AND #%11111110  ; No Y expansion
    STA $D01D
    LDA $D01B
    AND #%11111110  ; In front of data
    STA $D01B

    RTS

SHIP_SPRITE:
    .BYTE %00000000,%00011000,%00000000  ; Row 0
    .BYTE %00000000,%00111100,%00000000  ; Row 1
    .BYTE %00000000,%01111110,%00000000  ; Row 2
    ; ... 18 more rows
```

### Bitmap Pixel Plotting

```assembly
;=============================================================================
; PLOT PIXEL IN STANDARD BITMAP MODE
; X = pixel X coordinate (0-319)
; Y = pixel Y coordinate (0-199)
; A = pixel on (1) or off (0)
;=============================================================================

PLOT_PIXEL:
    STA PIXEL_STATE     ; Save on/off state

    ; Calculate block number = (Y / 8) * 40 + (X / 8)
    ; First: Y / 8
    LDA Y_COORD
    LSR                 ; Divide by 8
    LSR
    LSR
    STA TEMP_Y_BLOCK    ; Y block row (0-24)

    ; Multiply by 40
    ASL                 ; × 2
    STA BLOCK_LO
    ASL                 ; × 4
    ASL                 ; × 8
    CLC
    ADC BLOCK_LO        ; × 10
    ASL                 ; × 20
    ASL                 ; × 40
    STA BLOCK_LO        ; Low byte of block × 8

    ; Add X / 8
    LDA X_COORD
    LSR                 ; Divide by 8
    LSR
    LSR
    CLC
    ADC BLOCK_LO
    STA BLOCK_LO        ; Block number low

    ; Multiply block × 8 + (Y mod 8) = byte offset
    LDA BLOCK_LO
    ASL                 ; × 2
    ROL BLOCK_HI
    ASL                 ; × 4
    ROL BLOCK_HI
    ASL                 ; × 8
    ROL BLOCK_HI

    ; Add Y mod 8
    LDA Y_COORD
    AND #%00000111      ; Y mod 8
    CLC
    ADC BLOCK_LO
    STA BYTE_OFFSET_LO
    LDA BLOCK_HI
    ADC #$00
    STA BYTE_OFFSET_HI

    ; Calculate bit position = 7 - (X mod 8)
    LDA X_COORD
    AND #%00000111      ; X mod 8
    TAX
    LDA BIT_MASK_TABLE,X
    STA BIT_MASK

    ; Read-modify-write pixel
    LDY #$00
    LDA (BYTE_OFFSET_LO),Y

    LDX PIXEL_STATE
    BEQ TURN_OFF

TURN_ON:
    ORA BIT_MASK        ; Set bit
    JMP STORE

TURN_OFF:
    EOR #$FF
    AND BIT_MASK        ; Clear bit
    EOR #$FF

STORE:
    STA (BYTE_OFFSET_LO),Y
    RTS

BIT_MASK_TABLE:
    .BYTE %10000000, %01000000, %00100000, %00010000
    .BYTE %00001000, %00000100, %00000010, %00000001

; Variables
X_COORD:         .BYTE 0
Y_COORD:         .BYTE 0
PIXEL_STATE:     .BYTE 0
BLOCK_LO:        .BYTE 0
BLOCK_HI:        .BYTE 0
BYTE_OFFSET_LO:  .BYTE 0
BYTE_OFFSET_HI:  .BYTE 0
BIT_MASK:        .BYTE 0
TEMP_Y_BLOCK:    .BYTE 0
```

---

## Quick Reference Tables

### VIC-II Register Map (Partial - Part 1)

| Address | Dec | Register | Function |
|---------|-----|----------|----------|
| $D000 | 53248 | M0X | Sprite 0 X position (low 8 bits) |
| $D001 | 53249 | M0Y | Sprite 0 Y position |
| $D002 | 53250 | M1X | Sprite 1 X position |
| $D003 | 53251 | M1Y | Sprite 1 Y position |
| $D004 | 53252 | M2X | Sprite 2 X position |
| $D005 | 53253 | M2Y | Sprite 2 Y position |
| $D006 | 53254 | M3X | Sprite 3 X position |
| $D007 | 53255 | M3Y | Sprite 3 Y position |
| $D008 | 53256 | M4X | Sprite 4 X position |
| $D009 | 53257 | M4Y | Sprite 4 Y position |
| $D00A | 53258 | M5X | Sprite 5 X position |
| $D00B | 53259 | M5Y | Sprite 5 Y position |
| $D00C | 53260 | M6X | Sprite 6 X position |
| $D00D | 53261 | M6Y | Sprite 6 Y position |
| $D00E | 53262 | M7X | Sprite 7 X position |
| $D00F | 53263 | M7Y | Sprite 7 Y position |
| $D010 | 53264 | MSBX | Sprites X MSB (bit 8 of X position) |
| $D011 | 53265 | CR1 | Control Register 1 (ECM, BMM, DEN, RSEL, YSCROLL) |
| $D015 | 53269 | MEN | Sprite enable register |
| $D016 | 53270 | CR2 | Control Register 2 (MCM, CSEL, XSCROLL) |
| $D017 | 53271 | MYE | Sprite Y expansion |
| $D018 | 53272 | MP | Memory pointers (screen, character base) |
| $D01B | 53275 | MDP | Sprite-data priority |
| $D01C | 53276 | MMC | Sprite multicolor mode |
| $D01D | 53277 | MXE | Sprite X expansion |
| $D01E | 53278 | M-M | Sprite-sprite collision (clears on read) |
| $D01F | 53279 | M-D | Sprite-data collision (clears on read) |
| $D020 | 53280 | EC | Border color |
| $D021 | 53281 | B0C | Background color #0 |
| $D022 | 53282 | B1C | Background color #1 |
| $D023 | 53283 | B2C | Background color #2 |
| $D024 | 53284 | B3C | Background color #3 |
| $D025 | 53285 | MM0 | Sprite multicolor #0 |
| $D026 | 53286 | MM1 | Sprite multicolor #1 |
| $D027 | 53287 | M0C | Sprite 0 color |
| $D028 | 53288 | M1C | Sprite 1 color |
| $D029 | 53289 | M2C | Sprite 2 color |
| $D02A | 53290 | M3C | Sprite 3 color |
| $D02B | 53291 | M4C | Sprite 4 color |
| $D02C | 53292 | M5C | Sprite 5 color |
| $D02D | 53293 | M6C | Sprite 6 color |
| $D02E | 53294 | M7C | Sprite 7 color |

### Display Mode Summary

| Mode | MCM | BMM | ECM | Resolution | Colors/Block |
|------|-----|-----|-----|------------|--------------|
| Standard Character | 0 | 0 | 0 | 8×8 | 2 (1 fg + shared bg) |
| Multicolor Character | 1 | 0 | 0 | 4×8 | 4 (2 fg + 2 bg) |
| Extended Color | 0 | 0 | 1 | 8×8 | 2 (1 fg + 1 of 4 bg) |
| Standard Bitmap | 0 | 1 | 0 | 320×200 | 2 per 8×8 block |
| Multicolor Bitmap | 1 | 1 | 0 | 160×200 | 4 per 8×8 block |

### Color Code Table

| Value | Color | C64 Name |
|-------|-------|----------|
| 0 | Black | Black |
| 1 | White | White |
| 2 | Red | Red |
| 3 | Cyan | Cyan |
| 4 | Purple | Purple |
| 5 | Green | Green |
| 6 | Blue | Blue |
| 7 | Yellow | Yellow |
| 8 | Orange | Orange |
| 9 | Brown | Brown |
| 10 ($A) | Light Red | Light Red |
| 11 ($B) | Dark Gray | Dark Gray |
| 12 ($C) | Medium Gray | Medium Gray |
| 13 ($D) | Light Green | Light Green |
| 14 ($E) | Light Blue | Light Blue |
| 15 ($F) | Light Gray | Light Gray |

### Sprite Size Reference

| Mode | X Expand | Y Expand | Width | Height | Pixels |
|------|----------|----------|-------|--------|--------|
| Standard | 0 | 0 | 24 | 21 | 504 |
| Standard | 1 | 0 | 48 | 21 | 1008 |
| Standard | 0 | 1 | 24 | 42 | 1008 |
| Standard | 1 | 1 | 48 | 42 | 2016 |
| Multicolor | 0 | 0 | 12 | 21 | 252 |
| Multicolor | 1 | 0 | 24 | 21 | 504 |
| Multicolor | 0 | 1 | 12 | 42 | 504 |
| Multicolor | 1 | 1 | 24 | 42 | 1008 |

---

## Common Mistakes and Gotchas

### Character Mode

**Mistake:** Forgetting that color RAM is only 4 bits wide
```assembly
; WRONG
LDA #$11        ; Both nybbles set
STA $D800       ; Only lower nybble ($1) is stored

; RIGHT
LDA #$01        ; Single nybble
STA $D800
```

**Mistake:** Enabling ECM and MCM simultaneously
```assembly
; WRONG - undefined behavior
LDA $D011
ORA #%01000000  ; ECM = 1
STA $D011
LDA $D016
ORA #%00010000  ; MCM = 1 (DON'T DO THIS!)
STA $D016

; RIGHT - use one or the other
```

### Bitmap Mode

**Mistake:** Forgetting bitmap uses video matrix differently
```assembly
; Screen memory still needed in bitmap mode!
; It stores color data, not character pointers

; Set bitmap colors for block 0
LDA #$1E        ; White on light blue
STA $0400       ; Video matrix still used!
```

**Mistake:** Not clearing bitmap memory before use
```assembly
; Must initialize all 8000 bytes
; Otherwise you'll see random garbage
```

### Sprites

**Mistake:** Reading collision registers twice
```assembly
; WRONG
LDA $D01E       ; Read and clear
AND #$01
BEQ CHECK2
; ...
CHECK2:
    LDA $D01E   ; Returns $00 - already cleared!

; RIGHT
LDA $D01E       ; Read and clear
STA TEMP        ; Save immediately
```

**Mistake:** Forgetting X position is 9 bits
```assembly
; WRONG - sprite stuck at X < 256
LDA #200
STA $D000       ; Only sets lower 8 bits

; RIGHT - handle both bytes
LDA #200
STA $D000
LDA $D010
AND #%11111110  ; Clear MSB for sprite 0
STA $D010
```

**Mistake:** Not multiplying sprite pointer by 64
```assembly
; WRONG
LDA #$20        ; Sprite at $2000?
STA $07F8       ; NO! This points to $0800

; RIGHT
LDA #$80        ; $2000 / 64 = $80
STA $07F8
```

---

## Programming Checklists

### Setting Up Character Mode

- [ ] Select VIC bank via CIA #2 ($DD00)
- [ ] Set screen memory location ($D018 bits 7-4)
- [ ] Set character base location ($D018 bits 3-1)
- [ ] Clear mode bits: BMM=0, ECM=0, MCM=0 (if standard)
- [ ] Set background color(s) ($D021-$D024)
- [ ] Enable display (DEN=1 in $D011)

### Setting Up Bitmap Mode

- [ ] Select VIC bank via CIA #2
- [ ] Set bitmap location ($D018 bit 3: CB13)
- [ ] Clear bitmap memory (8000 bytes)
- [ ] Set color data in video matrix (1000 bytes)
- [ ] Set BMM=1 in $D011
- [ ] Set MCM bit if multicolor ($D016)
- [ ] Set background colors
- [ ] Enable display (DEN=1)

### Setting Up a Sprite

- [ ] Copy sprite data to memory (63 bytes)
- [ ] Calculate and set sprite pointer (address / 64)
- [ ] Enable sprite ($D015)
- [ ] Set X position (9 bits: $D000-$D00F + $D010)
- [ ] Set Y position ($D001-$D00F)
- [ ] Set color ($D027-$D02E)
- [ ] Set multicolor mode if needed ($D01C)
- [ ] Set expansion if needed ($D017, $D01D)
- [ ] Set priority ($D01B)

---

**★ Insight ─────────────────────────────────────**

The VIC-II's architecture reveals several clever design decisions:

1. **Shared address space**: The video matrix serves dual purposes - character pointers in character mode, color data in bitmap mode. This saves logic gates and makes mode switching efficient.

2. **Sprite priority system**: Fixed sprite-to-sprite priority (0 highest, 7 lowest) combined with configurable sprite-to-data priority provides flexibility without requiring complex circuitry for 8 independent priority levels.

3. **Multicolor special case**: The %01 bit pair not causing sprite collisions in multicolor mode is brilliant - it allows animated water, moving clouds, etc. without false collision detection, a common problem in sprite-based games.

─────────────────────────────────────────────────

---

## Advanced Display Control

### Row/Column Select (Display Window Size)

The VIC-II allows you to reduce the visible display window from the standard 25×40 character display to 24×38. This smaller window is primarily used in conjunction with scrolling to create smooth panning effects.

**Register Locations:**
- **RSEL** (Row Select): Bit 3 of $D011
- **CSEL** (Column Select): Bit 3 of $D016

**Display Window Sizes:**

| RSEL | Rows | CSEL | Columns | Use Case |
|------|------|------|---------|----------|
| 0 | 24 | 0 | 38 | Scrolling display |
| 1 | 25 | 1 | 40 | Standard display (default) |

**How it works:**
- When RSEL/CSEL = 0, the display window shrinks by one character row/column
- Characters adjacent to the border are now covered by the border color
- Display data format remains unchanged - only visibility changes
- Border expands to fill the gap

**Typical usage pattern:**
```assembly
; Standard display setup
LDA $D011
ORA #%00001000      ; RSEL=1 (25 rows)
STA $D011

LDA $D016
ORA #%00001000      ; CSEL=1 (40 columns)
STA $D016

; Scrolling display setup
LDA $D011
AND #%11110111      ; RSEL=0 (24 rows)
STA $D011

LDA $D016
AND #%11110111      ; CSEL=0 (38 columns)
STA $D016
```

### Scrolling

The VIC-II can scroll the display up to one full character space (8 pixels) in both horizontal and vertical directions. When combined with the smaller display window, this creates smooth scrolling games and demos.

**How smooth scrolling works:**

1. **Reduce display window** to 24×38 (RSEL=0, CSEL=0)
2. **Scroll display data** using scroll registers (values 0-7)
3. **Update screen memory** only when scrolling reaches a full character
4. **Reset scroll position** and continue

**Scroll Registers:**

| Bits | Register | Function | Range |
|------|----------|----------|-------|
| X2-X0 | $D016 bits 2-0 | Horizontal scroll position | 0-7 pixels |
| Y2-Y0 | $D011 bits 2-0 | Vertical scroll position | 0-7 pixels |

**Horizontal scroll example:**
```assembly
; Smooth horizontal scroll to the left
LDA XSCROLL        ; Current X scroll position (0-7)
SEC
SBC #1             ; Decrease by 1 pixel
BPL STORE_X        ; If still positive, just update

; Reached end of character - update screen memory
JSR SHIFT_SCREEN_LEFT
LDA #7             ; Reset to rightmost position

STORE_X:
    STA XSCROLL
    AND #%00000111
    ORA #%00001000  ; Keep CSEL=1 or adjust as needed
    STA $D016
```

**Vertical scroll example:**
```assembly
; Smooth vertical scroll upward
LDA YSCROLL        ; Current Y scroll position (0-7)
SEC
SBC #1
BPL STORE_Y

; Reached end of character row - update screen memory
JSR SHIFT_SCREEN_UP
LDA #7

STORE_Y:
    STA YSCROLL
    LDA $D011
    AND #%11111000  ; Clear Y scroll bits
    ORA YSCROLL     ; Set new Y scroll
    STA $D011
```

**Centering a fixed display:**
```assembly
; Center display horizontally
LDA #4              ; Offset by 4 pixels
AND #%00000111
ORA #%00001000      ; CSEL=1 for 40 columns
STA $D016

; Center display vertically
LDA $D011
AND #%11111000
ORA #3              ; Offset by 3 pixels
STA $D011
```

### Light Pen Support

The VIC-II has built-in light pen support that latches the screen position when a light pen detects the CRT beam.

**Light Pen Registers:**
- **LPX** ($D013): Horizontal position (8 MSB of 9-bit counter)
- **LPY** ($D014): Vertical position (raster line)

**Resolution:**
- **Horizontal**: 2-pixel resolution (512-state counter, only 8 MSB readable)
- **Vertical**: Single raster line resolution

**Important characteristics:**
- Triggered on **low-going edge** of light pen input
- Latches **only once per frame**
- Subsequent triggers in same frame have no effect
- Must take **3+ samples** and average for accuracy
- Accuracy depends on light pen hardware quality

**Light pen reading routine:**
```assembly
; Read light pen position with averaging
READ_PEN:
    LDX #0
    STX PEN_X
    STX PEN_Y

    LDX #5          ; Take 5 samples
SAMPLE_LOOP:
    LDA $D013       ; Read X position
    CLC
    ADC PEN_X
    STA PEN_X

    LDA $D014       ; Read Y position
    CLC
    ADC PEN_Y
    STA PEN_Y

    DEX
    BNE SAMPLE_LOOP

    ; Divide by 5 (approximate - shift right)
    LSR PEN_X
    LSR PEN_X       ; X / 4 (close enough)

    LSR PEN_Y
    LSR PEN_Y       ; Y / 4

    RTS

PEN_X:  .BYTE 0
PEN_Y:  .BYTE 0
```

**Curriculum notes:**
- Light pen support is rare in modern lessons (hardware not common)
- More relevant for historical understanding
- Similar principles apply to modern touch/pointing devices

### Raster Register (Dual Function)

The raster register at $D012 is one of the VIC-II's most important features. It serves two purposes: **reading** the current raster position and **writing** a target for raster interrupts.

**Raster Position:**
- **9-bit counter**: Bits 0-7 in $D012, bit 8 (RC8) in bit 7 of $D011
- **Visible display window**: Raster lines 51-251 ($033-$0FB)
- **Total raster lines**: 262 (NTSC) or 312 (PAL)

**Reading current raster position:**
```assembly
; Read current raster line (9 bits)
READ_RASTER:
    LDA $D011       ; Get bit 8
    AND #%10000000  ; Isolate RC8
    STA RASTER_HI   ; Save for later

    LDA $D012       ; Get bits 7-0
    STA RASTER_LO

    ; Combine into 16-bit value if needed
    LDA RASTER_HI
    BEQ LOW_RASTER  ; RC8=0, so raster < 256

    LDA RASTER_LO
    ORA #$01        ; Set bit 8
    STA RASTER_LO

LOW_RASTER:
    RTS

RASTER_HI: .BYTE 0
RASTER_LO: .BYTE 0
```

**Writing raster compare value:**

When you write to $D012 (and RC8), the value is stored for comparison. When the current raster line matches the stored value, the IRST interrupt latch is set.

```assembly
; Set raster interrupt for line 100
    LDA #100
    STA $D012       ; Set compare value (bits 7-0)

    LDA $D011
    AND #%01111111  ; Clear RC8 (line < 256)
    STA $D011

    ; Enable raster interrupt
    LDA #%00000001  ; ERST=1
    STA $D01A
```

**Preventing display flicker:**

By reading the raster register, you can time visual changes to occur during the non-visible area (vertical blank).

```assembly
; Wait for raster to reach safe area (bottom border)
WAIT_VBLANK:
    LDA $D012
    CMP #250        ; Near bottom of visible area
    BCC WAIT_VBLANK ; Branch if carry clear (A < 250)

    ; Safe to make visual changes here
    LDA #BLACK
    STA $D021       ; Change background color

    RTS
```

### Interrupt System

The VIC-II has four sources of interrupt, controlled by two registers: the **interrupt status register** ($D019) and the **interrupt enable register** ($D01A).

**Interrupt Sources:**

| Latch Bit | Enable Bit | Name | When Set |
|-----------|------------|------|----------|
| IRST (bit 0) | ERST (bit 0) | Raster | When raster line = compare value |
| IMDC (bit 1) | EMDC (bit 1) | Sprite-Data | First sprite-to-background collision |
| IMMC (bit 2) | EMMC (bit 2) | Sprite-Sprite | First sprite-to-sprite collision |
| ILP (bit 3) | ELP (bit 3) | Light Pen | Negative transition of LP input (once/frame) |
| IRQ (bit 7) | — | Main IRQ | Set when any enabled interrupt occurs |

**Interrupt Register ($D019) - Read:**
```assembly
; Check which interrupt occurred
    LDA $D019
    AND #%00000001
    BNE RASTER_IRQ

    LDA $D019
    AND #%00000010
    BNE SPRITE_DATA_COLLISION

    ; ... check other sources

RASTER_IRQ:
    ; Handle raster interrupt
    JSR RASTER_HANDLER

    ; CRITICAL: Clear the interrupt latch
    LDA #%00000001  ; Write 1 to clear IRST
    STA $D019

    JMP $EA81       ; Exit to KERNAL IRQ handler
```

**Interrupt Enable Register ($D01A) - Write:**
```assembly
; Enable raster interrupts only
    LDA #%00000001  ; ERST=1
    STA $D01A

; Enable multiple interrupt sources
    LDA #%00000111  ; ERST=1, EMDC=1, EMMC=1
    STA $D01A
```

**CRITICAL: Clearing interrupt latches**

Interrupt latches are cleared by **writing a 1** to the corresponding bit in $D019. This is unusual - most systems clear by writing 0.

```assembly
; WRONG - won't clear interrupt
    LDA #0
    STA $D019       ; Doesn't clear anything!

; RIGHT - write 1 to clear
    LDA #%00000001
    STA $D019       ; Clears IRST only

; Clear multiple interrupts
    LDA #%00001111  ; Clear all four sources
    STA $D019
```

**Selective interrupt handling:**

The write-1-to-clear feature allows you to handle specific interrupts without needing to remember which others are active.

```assembly
; Handle only raster interrupt, leave others pending
RASTER_HANDLER:
    ; Do raster effect work
    INC $D020       ; Flash border

    ; Clear ONLY the raster interrupt
    LDA #%00000001
    STA $D019       ; Other interrupts still pending

    RTS
```

**Setting up raster interrupts (complete example):**
```assembly
SETUP_IRQ:
    SEI             ; Disable interrupts during setup

    ; Set interrupt vector
    LDA #<IRQ_HANDLER
    STA $0314
    LDA #>IRQ_HANDLER
    STA $0315

    ; Set raster line
    LDA #100
    STA $D012

    LDA $D011
    AND #%01111111  ; RC8=0 (line < 256)
    STA $D011

    ; Enable raster interrupt
    LDA #%00000001
    STA $D01A

    ; Clear any pending interrupts
    LDA #%00001111
    STA $D019

    CLI             ; Re-enable interrupts
    RTS

IRQ_HANDLER:
    ; Save registers
    PHA
    TXA
    PHA
    TYA
    PHA

    ; Your effect code here
    INC $D020

    ; Acknowledge interrupt
    LDA #%00000001
    STA $D019

    ; Restore registers
    PLA
    TAY
    PLA
    TAX
    PLA

    RTI             ; Return from interrupt
```

---

## VIC-II System Architecture

### Dynamic RAM Refresh

The VIC-II includes a built-in DRAM refresh controller that maintains the C64's main memory without processor intervention.

**Refresh Characteristics:**
- **Refresh rate**: 5 row addresses per raster line
- **Maximum delay**: 2.02ms (128-row refresh), 3.66ms (256-row refresh)
- **Transparency**: Occurs during Phase 1 (processor uses Phase 2)
- **Signal generation**: VIC-II generates RAS/ and CAS/ directly

**Why this matters:**
- Refresh is **completely transparent** to both processor and programmer
- No performance penalty for memory refresh
- RAS/ and CAS/ generated for all accesses (refresh, video data, processor)
- External clock generation not required

**Curriculum notes:**
- Understanding refresh is Tier 3+ material
- Relevant when discussing why VIC-II "steals" cycles
- Important for cycle-exact timing and advanced raster effects

### System Bus Interface

The VIC-II shares the system bus with the 6510 processor through careful timing and control signals.

**Bus Sharing Principle:**

The 6510 uses the bus only during **Phase 2** (clock high). The VIC-II normally accesses memory during **Phase 1** (clock low), making most video operations transparent to the processor.

**Key Signals:**

| Signal | Direction | Function |
|--------|-----------|----------|
| **AEC** | Output | Address Enable Control - disables 6510 address drivers |
| **BA** | Output | Bus Available - signals Phase 2 access needed |
| **PH0** | Output | 1MHz clock for 6510 Phase 0 input |

**Timing Requirements:**
- Memory access window: **500ns** (half of 1MHz cycle)
- Includes address setup, data access, and data setup time
- All C64 memory must meet this timing

**AEC (Address Enable Control):**

```
Phase 1: AEC goes low
         ↓
    VIC-II drives address bus
         ↓
    VIC-II reads video data
         ↓
Phase 2: AEC goes high
         ↓
    6510 drives address bus
         ↓
    6510 executes instruction
```

**Normally transparent operations:**
- Character data fetches
- Bitmap data fetches
- DRAM refresh cycles

**BA (Bus Available):**

Some VIC-II operations require data faster than Phase 1 access allows:
- **Character pointer fetches** from video matrix (every 8th raster)
- **Sprite data fetches** (when sprites are active)

**BA timing sequence:**
```
Phase 1: BA goes low (warning signal)
         ↓
    [3 Phase 2 cycles allowed for 6510]
         ↓
Phase 2: AEC stays low (4th cycle)
         ↓
    VIC-II accesses memory during Phase 2
         ↓
    6510 is halted (RDY input low)
```

**Cycle stealing:**

```assembly
; This loop will run slower with sprites enabled
LOOP:
    INC $D020       ; 6 cycles normally
    DEX             ; 2 cycles
    BNE LOOP        ; 3 cycles

; With 8 sprites enabled, VIC-II steals cycles:
; - Character pointer fetches: 40 cycles per 8 raster lines
; - Sprite data fetches: 2-4 cycles per sprite per line
; = Total: Up to ~20% CPU time stolen during visible display
```

**BA connection:**
```
VIC-II BA pin → 6510 RDY pin
```

When BA goes low, the 6510 will complete its current instruction, then halt (stretching the next read cycle) until BA goes high again.

**Practical implications:**

```assembly
; Time-critical code should run during vertical blank
WAIT_SAFE:
    LDA $D012
    CMP #251        ; Past visible display
    BCC WAIT_SAFE

    ; Now safe - no sprite fetches, no badlines
    JSR TIME_CRITICAL_CODE
    RTS
```

### Memory Interface

The 6566 and 6567 variants differ in how they output addresses to memory.

**6566 (PAL) - Fully Decoded Addresses:**
- **13 address lines**: A13-A00 fully decoded
- Direct connection to system address bus
- Used in European C64 models
- Simpler external circuitry

**6567 (NTSC) - Multiplexed Addresses:**
- **Row Address Strobe (RAS/)**: A06-A00 carry bits A06-A00
- **Column Address Strobe (CAS/)**: A05-A00 carry bits A13-A08
- Direct connection to 64K dynamic RAMs
- Used in North American C64 models
- Requires external latching for ROM access

**6567 Address Multiplexing:**

```
RAS/ low:  A06-A00 = Row address (bits 6-0)
CAS/ low:  A05-A00 = Column address (bits 13-8)

Static outputs: A11-A07 (for ROM access)
```

**ROM access with 6567:**

```
Character ROM (2K):
- A11-A07: Static from VIC-II
- A06-A00: Must be latched during RAS/ low
```

**Why this matters for programmers:**

You don't need to worry about address multiplexing - it's handled in hardware. But it explains:
- Why custom hardware projects differ for NTSC/PAL
- Why some timing characteristics differ between versions
- How the VIC-II can address 16K video memory

### Processor Register Interface

The VIC-II's 47 registers appear as memory-mapped I/O at $D000-$D02E.

**Register Access Signals:**

| Signal | Direction | Function |
|--------|-----------|----------|
| **DB7-DB0** | Bidirectional | Data bus |
| **A05-A00** | Bidirectional | Register select (input), video address (output) |
| **CS/** | Input | Chip select (active low) |
| **R/W** | Input | Read/write control |

**Access Conditions:**

Registers can ONLY be accessed when:
- **AEC** = high (VIC-II not using bus)
- **PH0** = high (Phase 2)
- **CS/** = low (chip selected - usually $D000-$D0FF range)

**Data Bus (DB7-DB0):**

```assembly
; Write to register
    LDA #5          ; Value to write
    STA $D020       ; Write to border color
    ; CPU drives data bus

; Read from register
    LDA $D012       ; Read raster position
    ; VIC-II drives data bus
```

**Chip Select (CS/):**

The VIC-II is selected when the address is in range $D000-$D0FF:
```
Address $D000-$D02E: Valid VIC-II registers
Address $D02F-$D0FF: Mirror of $D000-$D02E
```

**Read/Write (R/W):**

```
R/W = 1 (high):  Read from VIC-II → Data bus
R/W = 0 (low):   Write to VIC-II ← Data bus
```

**Address Bus (A05-A00):**

During register access, these pins are **inputs** selecting which register:
```
$D000 → A05-A00 = %000000 → Register 0 (M0X)
$D012 → A05-A00 = %010010 → Register 18 (RASTER)
$D020 → A05-A00 = %100000 → Register 32 (BORDER)
```

During video access, these pins are **outputs** providing video memory addresses.

**Clock Output (PH0):**

The VIC-II generates the system's 1MHz clock:
```
8MHz crystal → ÷8 → 1MHz PH0 output → 6510 Phase 0 input
```

All system timing is derived from this clock. The 6510 MUST use this clock - the bus sharing scheme depends on perfect synchronization.

**Interrupt Output (IRQ/):**

```
IRQ/ = Open drain output (requires pull-up resistor)

Normal state:   IRQ/ = high (pulled up)
Interrupt:      IRQ/ = low (VIC-II pulls down)
```

Multiple devices can share the IRQ/ line (wired-OR):
```
VIC-II IRQ/  ──┐
               ├──── IRQ/ → 6510
CIA #1 IRQ/  ──┘
```

### Video Interface

The VIC-II generates two separate video signals that must be mixed externally.

**Video Output Signals:**

| Signal | Type | Function | Termination |
|--------|------|----------|-------------|
| **SYNC/LUM** | Open drain | Sync + Luminance | 500Ω pull-up |
| **COLOR** | Open source | Chrominance | 1000Ω to ground |

**SYNC/LUM (Luminance + Sync):**
- Horizontal and vertical sync pulses
- Luminance (brightness) information
- Open drain output (pulls low only)
- Requires external 500Ω pull-up resistor

**COLOR (Chrominance):**
- Color reference burst
- Color information for all display data
- Open source output (pulls high only)
- Requires 1000Ω to ground termination

**Signal Mixing:**

The two signals must be mixed to create composite video:
```
SYNC/LUM (500Ω to +5V)
         ↓
    [Mixer Circuit]
         ↓
COLOR (1000Ω to GND)
         ↓
    Composite Video → Monitor or RF Modulator
```

**Why two separate signals?**

This architecture allows:
- Clean separation of luminance and chrominance
- Easier generation in TTL logic
- Flexibility in video circuit design
- High-quality composite or S-Video output

**Curriculum notes:**
- Video interface details are for hardware designers
- Understanding helps when discussing video quality issues
- Relevant for advanced topics like video modifications

### Bus Activity Summary

The following table shows all possible bus states for the VIC-II:

| AEC | PH0 | CS/ | R/W | Action |
|-----|-----|-----|-----|--------|
| 0 | 0 | X | X | **Phase 1 fetch or refresh** |
| 0 | 1 | X | X | **Phase 2 fetch** (processor halted) |
| 1 | 0 | X | X | No action |
| 1 | 1 | 0 | 0 | **Write to selected register** |
| 1 | 1 | 0 | 1 | **Read from selected register** |
| 1 | 1 | 1 | X | No action |

**Reading the table:**

**Video accesses** (AEC=0):
- **Phase 1 (PH0=0)**: Character data, bitmap data, refresh
- **Phase 2 (PH0=1)**: Character pointers, sprite data (processor halted via BA)

**Processor accesses** (AEC=1):
- Only possible during Phase 2 (PH0=1)
- CS/ must be low to access registers
- R/W determines read or write

**Key insights:**

1. **Normal operation**: VIC-II uses Phase 1, processor uses Phase 2
2. **Cycle stealing**: VIC-II can also use Phase 2 (processor halted)
3. **Register access**: Only during Phase 2 when AEC is high
4. **Phase 1 processor**: Never happens - AEC is always low during Phase 1

---

## Color Reference

### VIC-II Color Codes

The VIC-II provides 16 colors, encoded as 4 bits (though only bits D3-D0 are used in most registers).

**Complete Color Table:**

| D4 | D3 | D2 | D1 | D0 | Hex | Dec | Color | Usage Notes |
|----|----|----|----|----|-----|-----|-------|-------------|
| 0 | 0 | 0 | 0 | 0 | $0 | 0 | **Black** | Common background |
| 0 | 0 | 0 | 0 | 1 | $1 | 1 | **White** | Common foreground |
| 0 | 0 | 0 | 1 | 0 | $2 | 2 | **Red** | Danger, fire |
| 0 | 0 | 0 | 1 | 1 | $3 | 3 | **Cyan** | Water, ice |
| 0 | 0 | 1 | 0 | 0 | $4 | 4 | **Purple** | Rare in games |
| 0 | 0 | 1 | 0 | 1 | $5 | 5 | **Green** | Grass, terrain |
| 0 | 0 | 1 | 1 | 0 | $6 | 6 | **Blue** | Sky, water |
| 0 | 0 | 1 | 1 | 1 | $7 | 7 | **Yellow** | Gold, light |
| 0 | 1 | 0 | 0 | 0 | $8 | 8 | **Orange** | Fire, sunset |
| 0 | 1 | 0 | 0 | 1 | $9 | 9 | **Brown** | Wood, earth |
| 0 | 1 | 0 | 1 | 0 | $A | 10 | **Light Red** | Highlights |
| 0 | 1 | 0 | 1 | 1 | $B | 11 | **Dark Grey** | Shadows |
| 0 | 1 | 1 | 0 | 0 | $C | 12 | **Medium Grey** | Metal |
| 0 | 1 | 1 | 0 | 1 | $D | 13 | **Light Green** | Bright grass |
| 0 | 1 | 1 | 1 | 0 | $E | 14 | **Light Blue** | Bright sky |
| 0 | 1 | 1 | 1 | 1 | $F | 15 | **Light Grey** | Highlights |

**Color Register Map:**

```assembly
; Border and background
$D020:  Border color
$D021:  Background #0 (main background)
$D022:  Background #1 (ECM/multicolor)
$D023:  Background #2 (ECM/multicolor)
$D024:  Background #3 (ECM only)

; Sprite colors
$D027:  Sprite 0 color
$D028:  Sprite 1 color
$D029:  Sprite 2 color
$D02A:  Sprite 3 color
$D02B:  Sprite 4 color
$D02C:  Sprite 5 color
$D02D:  Sprite 6 color
$D02E:  Sprite 7 color

; Shared multicolor sprite colors
$D025:  Sprite multicolor 0
$D026:  Sprite multicolor 1
```

**Standard color combinations:**

```assembly
; Classic C64 look
    LDA #LBLUE      ; 14
    STA $D020       ; Light blue border
    LDA #BLUE       ; 6
    STA $D021       ; Blue background

; High contrast text
    LDA #BLACK      ; 0
    STA $D020       ; Black border
    STA $D021       ; Black background
    ; White text in screen memory ($D800-$DBE7 = 1)

; Arcade-style
    LDA #BLACK      ; 0
    STA $D020
    STA $D021
    ; Colorful sprites and characters
```

**Color names for assembly:**
```assembly
BLACK   = 0
WHITE   = 1
RED     = 2
CYAN    = 3
PURPLE  = 4
GREEN   = 5
BLUE    = 6
YELLOW  = 7
ORANGE  = 8
BROWN   = 9
LTRED   = 10
DKGREY  = 11
GREY    = 12
LTGREEN = 13
LTBLUE  = 14
LTGREY  = 15
```

---

## Hardware Pin Configuration

### 6567 VIC-II Chip Pinout (NTSC)

The 6567 is the NTSC version used in North American C64 models. It features multiplexed address outputs for direct connection to dynamic RAM.

**40-pin DIP Package:**

```
                    6567
                  ┌─────┐
        DB₆   1 ──┤     ├── 40  Vcc (+5V)
        DB₅   2 ──┤     ├── 39  DB₇
        DB₄   3 ──┤     ├── 38  DB₈
        DB₃   4 ──┤     ├── 37  DB₉
        DB₂   5 ──┤     ├── 36  DB₁₀
        DB₁   6 ──┤     ├── 35  DB₁₁
        DB₀   7 ──┤     ├── 34  A₁₀
       IRQ/   8 ──┤     ├── 33  A₉
         LP   9 ──┤     ├── 32  A₈
        CS/  10 ──┤     ├── 31  A₇
        R/W  11 ──┤     ├── 30  A₆ ("1")
         BA  12 ──┤     ├── 29  A₅ (A₁₃)
        Vdd  13 ──┤     ├── 28  A₄ (A₁₂)
      COLOR  14 ──┤     ├── 27  A₃ (A₁₁)
      S/LUM  15 ──┤     ├── 26  A₂ (A₁₀)
        AEC  16 ──┤     ├── 25  A₁ (A₉)
        PH₀  17 ──┤     ├── 24  A₀ (A₈)
       RAS/  18 ──┤     ├── 23  A₁₁
       CAS/  19 ──┤     ├── 22  PHIN
        Vss  20 ──┤     ├── 21  PHCL
                  └─────┘

     (Multiplexed addresses in parentheses)
```

**Pin Descriptions (6567):**

| Pin | Name | Type | Function |
|-----|------|------|----------|
| 1-7, 35-39 | DB0-DB11 | I/O | Data bus (bidirectional, 12 bits for character ROM) |
| 8 | IRQ/ | Output | Interrupt request (open drain, active low) |
| 9 | LP | Input | Light pen input |
| 10 | CS/ | Input | Chip select (active low) |
| 11 | R/W | Input | Read/write control |
| 12 | BA | Output | Bus available (signals Phase 2 access needed) |
| 13 | Vdd | Power | Ground (0V) |
| 14 | COLOR | Output | Chrominance output (open source) |
| 15 | S/LUM | Output | Sync + Luminance output (open drain) |
| 16 | AEC | Output | Address enable control (active low) |
| 17 | PH0 | Output | Phase 0 clock output (1MHz) |
| 18 | RAS/ | Output | Row address strobe (active low) |
| 19 | CAS/ | Output | Column address strobe (active low) |
| 20 | Vss | Power | Ground (0V) |
| 21 | PHCL | Input | Color clock input |
| 22 | PHIN | Input | Clock input (8MHz crystal) |
| 23 | A11 | Output | Address line 11 (static) |
| 24-34 | A0-A10 | I/O | Address/data lines (multiplexed or static) |
| 40 | Vcc | Power | +5V supply |

**Multiplexed Address Pins (6567):**

| Pin | RAS/ Low | CAS/ Low | Static |
|-----|----------|----------|--------|
| 24 | A0 | A8 | — |
| 25 | A1 | A9 | — |
| 26 | A2 | A10 | — |
| 27 | A3 | A11 | — |
| 28 | A4 | A12 | — |
| 29 | A5 | A13 | — |
| 30 | A6 | "1" | — |
| 31-34 | — | — | A7-A10 |

**Notes for 6567:**
- Pins 24-29: Multiplexed row/column addresses for DRAM
- Pin 30 (A6): Outputs "1" during CAS/ (quirk of 6567 design)
- Pins 31-34, 23: Static address outputs (A7-A11) for ROM access
- Lower address bits (A0-A6) must be latched externally for ROM

### 6566 VIC-II Chip Pinout (PAL)

The 6566 is the PAL version used in European C64 models. It features fully decoded address outputs.

**40-pin DIP Package:**

```
                    6566
                  ┌─────┐
        DB₆   1 ──┤     ├── 40  Vcc (+5V)
        DB₅   2 ──┤     ├── 39  DB₇
        DB₄   3 ──┤     ├── 38  DB₈
        DB₃   4 ──┤     ├── 37  DB₉
        DB₂   5 ──┤     ├── 36  DB₁₀
        DB₁   6 ──┤     ├── 35  DB₁₁
        DB₀   7 ──┤     ├── 34  A₁₃
       IRQ/   8 ──┤     ├── 33  A₁₂
         LP   9 ──┤     ├── 32  A₁₁
        CS/  10 ──┤     ├── 31  A₁₀
        R/W  11 ──┤     ├── 30  A₉
         BA  12 ──┤     ├── 29  A₈
        Vdd  13 ──┤     ├── 28  A₇
      COLOR  14 ──┤     ├── 27  A₆
      S/LUM  15 ──┤     ├── 26  A₅
        AEC  16 ──┤     ├── 25  A₄
        PH₀  17 ──┤     ├── 24  A₃
       PHIN  18 ──┤     ├── 23  A₂
     PHCOL   19 ──┤     ├── 22  A₁
        Vss  20 ──┤     ├── 21  A₀
                  └─────┘
```

**Pin Descriptions (6566):**

| Pin | Name | Type | Function |
|-----|------|------|----------|
| 1-7, 35-39 | DB0-DB11 | I/O | Data bus (bidirectional, 12 bits for character ROM) |
| 8 | IRQ/ | Output | Interrupt request (open drain, active low) |
| 9 | LP | Input | Light pen input |
| 10 | CS/ | Input | Chip select (active low) |
| 11 | R/W | Input | Read/write control |
| 12 | BA | Output | Bus available (signals Phase 2 access needed) |
| 13 | Vdd | Power | Ground (0V) |
| 14 | COLOR | Output | Chrominance output (open source) |
| 15 | S/LUM | Output | Sync + Luminance output (open drain) |
| 16 | AEC | Output | Address enable control (active low) |
| 17 | PH0 | Output | Phase 0 clock output (1MHz) |
| 18 | PHIN | Input | Clock input (8MHz crystal) |
| 19 | PHCOL | Input | Color clock input |
| 20 | Vss | Power | Ground (0V) |
| 21-34 | A0-A13 | Output | Address lines (fully decoded, 14 bits) |
| 40 | Vcc | Power | +5V supply |

**Key Differences Between 6566 and 6567:**

| Feature | 6566 (PAL) | 6567 (NTSC) |
|---------|------------|-------------|
| **Address Output** | Fully decoded (A0-A13) | Multiplexed (RAS/CAS) |
| **Address Pins** | 14 dedicated pins (21-34) | 11 pins (multiplexed + static) |
| **RAS/CAS** | No RAS/CAS pins | Pins 18-19 |
| **ROM Interface** | Direct connection | Requires external latch for A0-A6 |
| **DRAM Interface** | Requires address demux | Direct connection |
| **Clock Pins** | PHIN (18), PHCOL (19) | PHIN (22), PHCL (21) |
| **Total Rasters** | 312 (PAL standard) | 262 (NTSC standard) |
| **Refresh Rate** | 50Hz | 60Hz |

**Hardware Design Implications:**

**For 6567 (NTSC):**
```
DRAM Connection:
- A0-A6 (pins 24-30) → DRAM address pins (row during RAS/, col during CAS/)
- A7-A11 (pins 31-34, 23) → Static address for ROM

Character ROM:
- A7-A11: Direct from VIC-II
- A0-A6: Must latch during RAS/ low
- Requires external 74LS373 or similar latch
```

**For 6566 (PAL):**
```
Any Memory Connection:
- A0-A13 (pins 21-34) → Direct to address bus
- No multiplexing, no latching needed
- Simpler circuit design
```

**Power Requirements:**

Both chips:
- **Vcc**: +5V ±5% (pin 40)
- **Vdd/Vss**: 0V ground (pins 13, 20)
- **Current draw**: ~300mA typical, 400mA maximum
- **Heat dissipation**: Requires adequate ventilation (chips run warm)

**Clock Inputs:**

**6567 (NTSC):**
- PHIN (pin 22): 8.18MHz crystal (NTSC colorburst × 2)
- PHCL (pin 21): Color reference clock

**6566 (PAL):**
- PHIN (pin 18): 7.88MHz crystal (PAL colorburst × 2)
- PHCOL (pin 19): Color reference clock

**Curriculum Notes:**

- Pin configurations are **reference material** for hardware projects
- Understanding pinouts helps when:
  - Building custom C64 hardware expansions
  - Debugging faulty hardware
  - Designing VIC-II replacement/enhancement projects
  - Understanding NTSC vs PAL differences
- **Not required** for pure software programming
- **Relevant** for Tier 4 advanced topics (custom hardware, FPGA recreations)

**Common Hardware Modifications:**

```
Video Output Enhancement:
- S/LUM (pin 15) → S-Video luminance (via buffer)
- COLOR (pin 14) → S-Video chrominance (via buffer)
- Improved video quality vs composite

External Sprite Generator:
- BA (pin 12) → Monitor for sprite timing
- AEC (pin 16) → Coordinate bus access
- DB0-DB11 → Inject sprite data during DMA
```

---

## Complete Register Map

### VIC-II Register Summary ($D000-$D02E)

The VIC-II has 47 registers mapped to memory addresses $D000-$D02E. These registers control all aspects of video generation.

**Register Map Table:**

| Address | Hex | Register | DB7 | DB6 | DB5 | DB4 | DB3 | DB2 | DB1 | DB0 | Description |
|---------|-----|----------|-----|-----|-----|-----|-----|-----|-----|-----|-------------|
| 00 | $D000 | M0X | M0X7 | M0X6 | M0X5 | M0X4 | M0X3 | M0X2 | M0X1 | M0X0 | MOB 0 X-Position |
| 01 | $D001 | M0Y | M0Y7 | M0Y6 | M0Y5 | M0Y4 | M0Y3 | M0Y2 | M0Y1 | M0Y0 | MOB 0 Y-Position |
| 02 | $D002 | M1X | M1X7 | M1X6 | M1X5 | M1X4 | M1X3 | M1X2 | M1X1 | M1X0 | MOB 1 X-Position |
| 03 | $D003 | M1Y | M1Y7 | M1Y6 | M1Y5 | M1Y4 | M1Y3 | M1Y2 | M1Y1 | M1Y0 | MOB 1 Y-Position |
| 04 | $D004 | M2X | M2X7 | M2X6 | M2X5 | M2X4 | M2X3 | M2X2 | M2X1 | M2X0 | MOB 2 X-Position |
| 05 | $D005 | M2Y | M2Y7 | M2Y6 | M2Y5 | M2Y4 | M2Y3 | M2Y2 | M2Y1 | M2Y0 | MOB 2 Y-Position |
| 06 | $D006 | M3X | M3X7 | M3X6 | M3X5 | M3X4 | M3X3 | M3X2 | M3X1 | M3X0 | MOB 3 X-Position |
| 07 | $D007 | M3Y | M3Y7 | M3Y6 | M3Y5 | M3Y4 | M3Y3 | M3Y2 | M3Y1 | M3Y0 | MOB 3 Y-Position |
| 08 | $D008 | M4X | M4X7 | M4X6 | M4X5 | M4X4 | M4X3 | M4X2 | M4X1 | M4X0 | MOB 4 X-Position |
| 09 | $D009 | M4Y | M4Y7 | M4Y6 | M4Y5 | M4Y4 | M4Y3 | M4Y2 | M4Y1 | M4Y0 | MOB 4 Y-Position |
| 10 | $D00A | M5X | M5X7 | M5X6 | M5X5 | M5X4 | M5X3 | M5X2 | M5X1 | M5X0 | MOB 5 X-Position |
| 11 | $D00B | M5Y | M5Y7 | M5Y6 | M5Y5 | M5Y4 | M5Y3 | M5Y2 | M5Y1 | M5Y0 | MOB 5 Y-Position |
| 12 | $D00C | M6X | M6X7 | M6X6 | M6X5 | M6X4 | M6X3 | M6X2 | M6X1 | M6X0 | MOB 6 X-Position |
| 13 | $D00D | M6Y | M6Y7 | M6Y6 | M6Y5 | M6Y4 | M6Y3 | M6Y2 | M6Y1 | M6Y0 | MOB 6 Y-Position |
| 14 | $D00E | M7X | M7X7 | M7X6 | M7X5 | M7X4 | M7X3 | M7X2 | M7X1 | M7X0 | MOB 7 X-Position |
| 15 | $D00F | M7Y | M7Y7 | M7Y6 | M7Y5 | M7Y4 | M7Y3 | M7Y2 | M7Y1 | M7Y0 | MOB 7 Y-Position |
| 16 | $D010 | MX8 | M7X8 | M6X8 | M5X8 | M4X8 | M3X8 | M2X8 | M1X8 | M0X8 | MSB of X-position |
| 17 | $D011 | — | RC8 | ECM | BMM | DEN | RSEL | Y2 | Y1 | Y0 | Raster register / Control |
| 18 | $D012 | — | RC7 | RC6 | RC5 | RC4 | RC3 | RC2 | RC1 | RC0 | Raster counter |
| 19 | $D013 | — | LPX7 | LPX6 | LPX5 | LPX4 | LPX3 | LPX2 | LPX1 | — | Light Pen X |
| 20 | $D014 | — | LPY7 | LPY6 | LPY5 | LPY4 | LPY3 | LPY2 | LPY1 | LPY0 | Light Pen Y |
| 21 | $D015 | — | M7E | M6E | M5E | M4E | M3E | M2E | M1E | M0E | MOB Enable |
| 22 | $D016 | — | — | RES | MCM | CSEL | X2 | X1 | X0 | — | Control register |
| 23 | $D017 | — | M7YE | M6YE | M5YE | M4YE | M3YE | M2YE | M1YE | M0YE | MOB Y-expand |

**Register Map Table (continued):**

| Address | Hex | Register | VM13 | VM12 | VM11 | VM10 | CB13 | CB12 | CB11 | — | Description |
|---------|-----|----------|------|------|------|------|------|------|------|---|-------------|
| 24 | $D018 | — | VM13 | VM12 | VM11 | VM10 | CB13 | CB12 | CB11 | — | Memory Pointers |
| 25 | $D019 | — | IRQ | — | ILP | IMMC | IMDC | IRST | — | — | Interrupt Register |
| 26 | $D01A | — | — | — | ELP | EMMC | EMDC | ERST | — | — | Enable Interrupt |
| 27 | $D01B | — | M7DP | M6DP | M5DP | M4DP | M3DP | M2DP | M1DP | — | MOB-DATA Priority |
| 28 | $D01C | — | M7MC | M6MC | M5MC | M4MC | M3MC | M2MC | M1MC | M0MC | MOB Multi-color Sel |
| 29 | $D01D | — | M7XE | M6XE | M5XE | M4XE | M3XE | M2XE | M1XE | M0XE | MOB X-expand |
| 30 | $D01E | — | M7M | M6M | M5M | M4M | M3M | M2M | M1M | M0M | MOB-MOB Collision |
| 31 | $D01F | — | M7D | M6D | M5D | M4D | M3D | M2D | M1D | M0D | MOB-DATA Collision |
| 32 | $D020 | — | — | — | — | EC3 | EC2 | EC1 | EC0 | — | Exterior Color |
| 33 | $D021 | — | — | — | — | B0C3 | B0C2 | B0C1 | B0C0 | — | Background #0 Color |
| 34 | $D022 | — | — | — | — | B1C3 | B1C2 | B1C1 | B1C0 | — | Background #1 Color |
| 35 | $D023 | — | — | — | — | B2C3 | B2C2 | B2C1 | B2C0 | — | Background #2 Color |
| 36 | $D024 | — | — | — | — | B3C3 | B3C2 | B3C1 | B3C0 | — | Background #3 Color |
| 37 | $D025 | — | — | — | — | MM03 | MM02 | MM01 | MM00 | — | MOB Multi-color #0 |
| 38 | $D026 | — | — | — | — | MM13 | MM12 | MM11 | MM10 | — | MOB Multi-color #1 |
| 39 | $D027 | — | — | — | — | M0C3 | M0C2 | M0C1 | M0C0 | — | MOB 0 Color |
| 40 | $D028 | — | — | — | — | M1C3 | M1C2 | M1C1 | M1C0 | — | MOB 1 Color |
| 41 | $D029 | — | — | — | — | M2C3 | M2C2 | M2C1 | M2C0 | — | MOB 2 Color |
| 42 | $D02A | — | — | — | — | M3C3 | M3C2 | M3C1 | M3C0 | — | MOB 3 Color |
| 43 | $D02B | — | — | — | — | M4C3 | M4C2 | M4C1 | M4C0 | — | MOB 4 Color |
| 44 | $D02C | — | — | — | — | M5C3 | M5C2 | M5C1 | M5C0 | — | MOB 5 Color |
| 45 | $D02D | — | — | — | — | M6C3 | M6C2 | M6C1 | M6C0 | — | MOB 6 Color |
| 46 | $D02E | — | — | — | — | M7C3 | M7C2 | M7C1 | M7C0 | — | MOB 7 Color |

**Note:** "—" in bit columns indicates unused bits (typically read as 1).

### Register Details by Function

**Sprite Position Registers ($D000-$D00F, $D010):**
- **M0X-M7X**: 8-bit X position (low byte)
- **M0Y-M7Y**: 8-bit Y position (0-255)
- **MX8**: 9th bit for each sprite's X position (enables 0-511 range)

**Control Register 1 ($D011):**
- **RC8**: Raster compare bit 8
- **ECM**: Extended Color Mode (1=on)
- **BMM**: Bitmap Mode (1=on)
- **DEN**: Display Enable (1=on, 0=blank screen)
- **RSEL**: Row Select (1=25 rows, 0=24 rows)
- **Y2-Y0**: Vertical scroll position (0-7)

**Control Register 2 ($D016):**
- **RES**: Reset (normally unused)
- **MCM**: Multicolor Mode (1=on)
- **CSEL**: Column Select (1=40 columns, 0=38 columns)
- **X2-X0**: Horizontal scroll position (0-7)

**Memory Pointers ($D018):**
- **VM13-VM10**: Video Matrix base address (bits 13-10)
- **CB13-CB11**: Character/Bitmap base address (bits 13-11)

**Interrupt Registers ($D019, $D01A):**
- **$D019**: Interrupt status (read to check, write 1 to clear)
- **$D01A**: Interrupt enable (write 1 to enable)
- **IRQ**: Any interrupt occurred
- **ILP/ELP**: Light pen interrupt
- **IMMC/EMMC**: Sprite-sprite collision interrupt
- **IMDC/EMDC**: Sprite-background collision interrupt
- **IRST/ERST**: Raster compare interrupt

**Sprite Control Registers:**
- **$D015**: Sprite enable (1 bit per sprite)
- **$D01B**: Sprite priority (1=behind background, 0=in front)
- **$D01C**: Sprite multicolor mode (1=multicolor, 0=hires)
- **$D01D**: Sprite X expansion (1=double width)
- **$D017**: Sprite Y expansion (1=double height)

**Collision Registers (Read-Only):**
- **$D01E**: Sprite-sprite collision (clears on read)
- **$D01F**: Sprite-background collision (clears on read)

**Color Registers ($D020-$D02E):**
- **$D020**: Border color (exterior color)
- **$D021**: Background color #0 (main background)
- **$D022**: Background color #1 (multicolor/ECM)
- **$D023**: Background color #2 (multicolor/ECM)
- **$D024**: Background color #3 (ECM only)
- **$D025**: Sprite multicolor #0 (shared)
- **$D026**: Sprite multicolor #1 (shared)
- **$D027-$D02E**: Individual sprite colors (0-7)

---

## Timing Specifications

### 6567 VIC-II Timing Limits (NTSC)

Critical timing specifications for hardware design and cycle-exact programming.

**Clock Timing (all values in nanoseconds):**

| Specification | Min | Typical | Max |
|--------------|-----|---------|-----|
| **Clock Output** | | | |
| Clock out high | 465 | 484 | 500 |
| Clock out low | 475 | 494 | 510 |
| **RAS/CAS Timing** | | | |
| Clock to RAS/ low | 150 | 171 | 190 |
| Clock to RAS/ high | 20 | 35 | 50 |
| RAS/ low to CAS/ low | 25 | 46 | 65 |
| Clock to CAS/ high | 15 | 25 | 35 |
| **Bus Control** | | | |
| Clock to AEC high/low | 15 | 33 | 50 |
| BA from Phase 0 | 100 | 230 | 300 |

**Data Timing (nanoseconds):**

| Specification | Min | Typical | Max |
|--------------|-----|---------|-----|
| **Data Output** | | | |
| Data out from CAS/ | — | 184 | 220 |
| Data release from Ph0 | 80 | 113 | 135 |
| **Data Input** | | | |
| Data in setup to Ph0 | 60 | 42 | — |
| Data in hold from Ph0 | 45 | 24 | — |
| Color data setup | 45 | 30 | — |
| Color data hold | 0 | -17 | — |

**Address Timing (nanoseconds):**

| Specification | Min | Typical | Max |
|--------------|-----|---------|-----|
| **Address Input** | | | |
| Address in to RAS/ setup | 25 | 14 | — |
| Address in to RAS/ hold | 0 | -15 | — |
| **Address Output** | | | |
| Address out to RAS/ setup | 35 | 48 | — |
| Address out to RAS/ hold | 30 | 36 | 45 |
| Address out from Ph0 | — | 85 | 97 |
| Address out to CAS↑ hold | 20 | 37 | 50 |

**Clock Input Timing (nanoseconds):**

| Specification | Min | Typical | Max |
|--------------|-----|---------|-----|
| Ph in + pulse width | 50 | 43 | — |
| Ph in - pulse width | 65 | 58 | — |

**Voltage Levels:**

| Parameter | Min | Typical | Max | Description |
|-----------|-----|---------|-----|-------------|
| **Input Levels** | | | | |
| Vil | — | 1.23 | 0.80 | Input low voltage |
| Vih | 2.20 | 1.91 | — | Input high voltage |
| **Output Levels** | | | | |
| Vol | — | 0.52 | 0.55 | Output low voltage |
| Voh | 2.40 | 3.03 | — | Output high voltage |

**Timing Notes:**

1. **Negative values** indicate hold time extends beyond the reference edge
2. All timing values assume **+5V ±5%** power supply
3. **Typical** values are for 25°C ambient temperature
4. **Min/Max** values cover -40°C to +85°C temperature range
5. Timing critical for:
   - Custom DRAM configurations
   - Cycle-exact effects
   - Hardware expansions
   - FPGA implementations

**Memory Cycle Timing:**

```
       |←─────── 500ns (typical) ────────→|

Ph0:   ___________╱‾‾‾‾‾‾‾‾‾‾‾‾╲___________
                  ↑            ↑
                  |            |
RAS/:  ‾‾‾‾╲______            _______╱‾‾‾‾
          ↑ ↑                      ↑
          | 171ns (Clock to RAS/)  |
          |                        35ns (Clock to RAS↑)

CAS/:  ‾‾‾‾‾‾‾╲______        _______╱‾‾‾‾‾‾
              ↑ ↑                  ↑
              | 46ns (RAS to CAS)  |
              |                    25ns (Clock to CAS↑)

Address: [Row Address]    [Column Address]
Data:    ───────────────────────[Valid]────
                                ↑
                                184ns (Data from CAS)
```

**BA Signal Timing:**

```
Phase 1: BA goes low (warning)
         ↓
    [3 Phase 2 cycles for CPU to finish]
         ↓
Phase 2: AEC stays low (VIC-II takes bus)

Typical BA early warning: 230ns before bus needed
```

**Practical Timing Considerations:**

**For programmers:**
```assembly
; VIC-II steals cycles during badlines
; Each badline: 40-43 stolen cycles

LOOP:
    INC $D020       ; Normally 6 cycles
    ; But can take 6-12 cycles if badline occurs
    ; Plan critical timing for raster areas without badlines
```

**For hardware designers:**
```
Memory speed requirements:
- 500ns total cycle time
- <220ns access time from CAS/
- Fast enough for VIC-II video + CPU sharing
- Typical: 200ns-250ns DRAM chips
```

---

## Appendix: Quick Reference

### Essential Registers for Common Tasks

**Setting up a sprite:**
```assembly
LDA #100
STA $D000       ; X position (low byte)
STA $D001       ; Y position

LDA $D010
ORA #%00000001  ; Set bit 0 for sprite 0 MSB
STA $D010       ; X position bit 8

LDA #1
STA $D015       ; Enable sprite 0

LDA #RED
STA $D027       ; Set color to red

LDA #$80        ; Sprite data at $2000
STA $07F8       ; Set sprite 0 pointer
```

**Setting up raster interrupt:**
```assembly
SEI
LDA #<IRQ_HANDLER
STA $0314
LDA #>IRQ_HANDLER
STA $0315

LDA #100
STA $D012       ; Raster line

LDA $D011
AND #%01111111
STA $D011       ; Clear RC8

LDA #%00000001
STA $D01A       ; Enable raster IRQ

CLI
```

**Switching to bitmap mode:**
```assembly
LDA $D011
ORA #%00100000  ; BMM=1
STA $D011

LDA $D018
ORA #%00001000  ; Bitmap at $2000
STA $D018
```

**Reading collisions:**
```assembly
LDA $D01F       ; Read sprite-background collision
BEQ NO_COLLISION
; Handle collision
; Note: Clears on read!
STA COLLISION_COPY  ; Save if needed

NO_COLLISION:
```

### Register Address Constants

```assembly
; Sprite positions
M0X     = $D000
M0Y     = $D001
; ... M1X-M7Y at $D002-$D00F
MX8     = $D010

; Control
CR1     = $D011
RASTER  = $D012
CR2     = $D016
MEMAP   = $D018

; Interrupts
INTFLAG = $D019
INTMASK = $D01A

; Sprite control
SPREN   = $D015
SPRBG   = $D01B
SPRMC   = $D01C
SPREX   = $D01D
SPREY   = $D017

; Collisions
SPRSP   = $D01E
SPRBG   = $D01F

; Colors
BORDER  = $D020
BG0     = $D021
BG1     = $D022
BG2     = $D023
BG3     = $D024
SPRMC0  = $D025
SPRMC1  = $D026
SPR0COL = $D027
; ... SPR1COL-SPR7COL at $D028-$D02E
```

---

## Document Completion

This reference document now provides complete coverage of the 6566/6567 VIC-II video interface chip, synthesized from the C64 Programmer's Reference Guide Appendix N.

**Coverage includes:**
- ✅ Character display modes (standard, multicolor, extended color)
- ✅ Bitmap graphics modes (standard and multicolor)
- ✅ Sprite system (MOBs) with all features
- ✅ Memory organization and addressing
- ✅ Display control (scrolling, borders, resolution)
- ✅ Interrupt system (raster, collision, light pen)
- ✅ System architecture (bus sharing, cycle stealing)
- ✅ Hardware pinouts (6566 PAL and 6567 NTSC)
- ✅ Complete register map (all 47 registers)
- ✅ Timing specifications (for hardware design)

**Usage for curriculum development:**
- **Tier 2 (BASIC Hardware)**: Color registers, basic POKE usage
- **Tier 3 (Assembly Fundamentals)**: Register programming, sprites, scrolling
- **Tier 4 (Advanced Techniques)**: Raster interrupts, cycle-exact timing, hardware tricks
- **Reference Material**: Complete technical specification for all skill levels

---

*End of VIC-II Reference Document*
