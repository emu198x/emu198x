# VIC-II Quick Reference - Programming Essentials

**Purpose:** Quick lookup for lesson creation - screen, colors, sprites, scrolling
**For comprehensive technical details:** See VIC-II-REFERENCE.md
**Audience:** Curriculum designers creating C64 graphics/game lessons

---

## What You Need to Know

The VIC-II controls all video display on the C64:
- Text/character modes
- Bitmap graphics
- Sprites (8 hardware sprites)
- Colors (16 colors total)
- Scrolling and raster effects

**Registers:** $D000-$D02E (53248-53294 decimal)

---

## Essential Registers Quick Lookup

### Most Common (Beginners)

| Address | Dec | Register | Purpose |
|---------|-----|----------|---------|
| **$D020** | 53280 | Border color | Color of screen border (0-15) |
| **$D021** | 53281 | Background color | Main background color (0-15) |
| **$D015** | 53269 | Sprite enable | Turn sprites on/off (8 bits) |
| **$D000-$D00F** | 53248-53263 | Sprite positions | X/Y coords for 8 sprites |
| **$D010** | 53264 | Sprite X MSB | 9th bit for X position (0-511) |
| **$D027-$D02E** | 53287-53294 | Sprite colors | Individual colors for sprites 0-7 |

### Screen Control

| Address | Dec | Register | Purpose |
|---------|-----|----------|---------|
| **$D018** | 53272 | Memory setup | Where screen and charset located |
| **$D011** | 53265 | Screen control 1 | Display mode, raster IRQ, scrolling |
| **$D016** | 53270 | Screen control 2 | Multicolor, horizontal scroll |

### Advanced (Intermediate+)

| Address | Dec | Register | Purpose |
|---------|-----|----------|---------|
| **$D012** | 53266 | Raster line | Current raster line (0-255) |
| **$D019** | 53273 | IRQ status | Raster/collision interrupts |
| **$D01A** | 53274 | IRQ enable | Enable raster interrupts |
| **$D01B** | 53275 | Sprite priority | Sprite behind/in front of background |
| **$D01C** | 53276 | Sprite multicolor | Multicolor mode per sprite |
| **$D01D** | 53277 | Sprite X expand | Double width sprites |
| **$D017** | 53271 | Sprite Y expand | Double height sprites |
| **$D01E** | 53278 | Sprite-sprite collision | Sprite collision detection |
| **$D01F** | 53279 | Sprite-data collision | Sprite hits background |

---

## Colors Quick Reference

### The 16 C64 Colors

```
 0 = Black        4 = Purple       8 = Orange      12 = Medium Gray
 1 = White        5 = Green        9 = Brown       13 = Light Green
 2 = Red          6 = Blue        10 = Light Red   14 = Light Blue
 3 = Cyan         7 = Yellow      11 = Dark Gray   15 = Light Gray
```

### Common Color Combinations

**Classic C64:**
```assembly
LDA #6          ; Blue
STA $D021       ; Background
LDA #14         ; Light blue
STA $D020       ; Border
```

**Retro Green (like monitor):**
```assembly
LDA #0          ; Black background
STA $D021
LDA #5          ; Green
; Use for text color in Color RAM
```

---

## Screen Memory Basics

### Default Layout

| What | Address | Size | Purpose |
|------|---------|------|---------|
| **Screen RAM** | $0400-$07E7 | 1000 bytes | What character at each position |
| **Color RAM** | $D800-$DBE7 | 1000 bytes | Color of each character (0-15) |

**Formula:** Position = $0400 + (row × 40) + column

### Writing to Screen

```assembly
; Put 'A' (screen code 1) at top-left corner
LDA #1
STA $0400       ; Screen memory

; Make it white
LDA #1
STA $D800       ; Color RAM
```

### Screen Codes vs PETSCII

**Important:** Screen memory uses SCREEN CODES, not PETSCII codes!

```
PETSCII 'A' = 65 ($41)
Screen code for 'A' = 1 ($01)
```

See PETSCII-SCREEN-CODES.md for conversion table.

---

## Sprites (Movable Object Blocks)

### Sprite Basics

- **8 sprites** total (numbered 0-7)
- **24×21 pixels** each (can double to 48×42)
- **Two colors** in hires mode, **three colors** in multicolor mode
- **Resolution-independent** from screen

### Essential Sprite Registers

```assembly
; Enable sprite 0
LDA #%00000001  ; Bit 0 = sprite 0
STA $D015       ; Sprite enable register

; Position sprite 0 at X=100, Y=100
LDA #100
STA $D000       ; Sprite 0 X position (low byte)
STA $D001       ; Sprite 0 Y position

; Set sprite 0 color to red (2)
LDA #2
STA $D027       ; Sprite 0 color

; Point sprite 0 to data at $2000
; Sprite pointer = data address / 64
; $2000 / 64 = 128
LDA #128
STA $07F8       ; Sprite 0 pointer (last 8 bytes of screen)
```

### Sprite Pointer Locations

**If screen is at $0400:**
```
Sprite 0 pointer: $07F8 (2040)
Sprite 1 pointer: $07F9 (2041)
Sprite 2 pointer: $07FA (2042)
...
Sprite 7 pointer: $07FF (2047)
```

**Formula:** Screen base + $03F8 + sprite number

### Sprite Data Structure

Each sprite requires **64 bytes** (63 for image + 1 padding):
```
Bytes 0-62: Sprite image data (3 bytes per row × 21 rows)
Byte 63: Unused (padding)
```

**Simple sprite example (arrow pointing right):**
```assembly
SPRITE_DATA:
    .byte $00,$7E,$00  ; Row 0:  .......########.........
    .byte $01,$FF,$80  ; Row 1:  .....############.......
    .byte $03,$FF,$C0  ; Row 2:  ....##############......
    .byte $07,$FF,$E0  ; Row 3:  ...################.....
    ; ... 17 more rows
    .byte $00          ; Padding
```

### Sprite X Position Beyond 255

X position is 9 bits (0-511):
```assembly
; Position sprite 0 at X=300 (beyond 255)
LDA #44         ; 300 - 256 = 44 (low byte)
STA $D000       ; X position low byte

LDA $D010       ; Read MSB register
ORA #%00000001  ; Set bit 0 (sprite 0 MSB)
STA $D010       ; Now X = 256 + 44 = 300
```

---

## Memory Setup Register ($D018)

This register controls where the VIC-II looks for screen and character data.

### $D018 Bit Layout

```
Bits 7-4 (VM): Video Matrix base address / 1024
Bits 3-1 (CB): Character/Bitmap base address / 2048
Bit 0: Unused
```

### Common Configurations

**Default (screen at $0400, charset at $1000):**
```assembly
LDA #$14        ; %00010100
STA $D018       ; VM=1 ($0400), CB=2 ($1000)
```

**Screen at $0400, custom charset at $2000:**
```assembly
LDA #$18        ; %00011000
STA $D018       ; VM=1 ($0400), CB=4 ($2000)
```

**Formula:**
```
Value = (Screen / 1024) * 16 + (Charset / 2048) * 2
```

### Common Screen Locations

| Screen Address | VM bits | Value for $D018 (charset at $1000) |
|----------------|---------|-------------------------------------|
| $0400 | 0001 (1) | $14 (default) |
| $0800 | 0010 (2) | $24 |
| $0C00 | 0011 (3) | $34 |
| $1000 | 0100 (4) | $44 |

---

## Display Modes

### Text/Character Modes

| Mode | $D011 BMM | $D011 ECM | $D016 MCM | Colors per char |
|------|-----------|-----------|-----------|-----------------|
| **Standard text** | 0 | 0 | 0 | 2 (fg + bg) |
| **Multicolor text** | 0 | 0 | 1 | 4 (but limited) |
| **Extended color** | 0 | 1 | 0 | 4 backgrounds |

### Bitmap Modes

| Mode | $D011 BMM | $D016 MCM | Resolution | Colors |
|------|-----------|-----------|------------|--------|
| **Standard bitmap** | 1 | 0 | 320×200 | 2 per 8×8 cell |
| **Multicolor bitmap** | 1 | 1 | 160×200 | 4 per 8×8 cell |

### Enabling Bitmap Mode

```assembly
; Enter standard bitmap mode
LDA $D011
ORA #%00100000  ; Set BMM (bit 5)
STA $D011

; Set bitmap base and screen (example: bitmap at $2000)
LDA #$08        ; Bitmap base / 2048 = 1, VM = 0
STA $D018
```

---

## Raster Interrupts (IRQ)

Raster IRQs trigger at specific screen lines for effects.

### Basic Raster IRQ Setup

```assembly
; Trigger IRQ at raster line 100
SEI             ; Disable interrupts

LDA #100        ; Raster line (low 8 bits)
STA $D012       ; Raster compare

LDA $D011       ; Read control register
AND #%01111111  ; Clear bit 7 (raster bit 8)
STA $D011       ; Write back

LDA #%00000001  ; Enable raster IRQ
STA $D01A       ; IRQ enable

; Install custom IRQ handler
LDA #<IRQ_HANDLER
STA $0314
LDA #>IRQ_HANDLER
STA $0315

CLI             ; Re-enable interrupts

; IRQ Handler:
IRQ_HANDLER:
    ; Acknowledge VIC-II interrupt
    LDA $D019       ; Read interrupt register
    STA $D019       ; Write back to clear

    ; Your effect here (e.g., change border color)
    INC $D020

    ; Jump to KERNAL IRQ routine
    JMP $EA31
```

**Critical:** Always acknowledge interrupt by reading and writing $D019!

---

## Common Patterns for Lessons

### Pattern 1: Basic Sprite Setup and Movement

```assembly
; Initialize sprite 0
LDA #%00000001
STA $D015       ; Enable sprite 0

LDA #50         ; Starting X
STA $D000
LDA #100        ; Starting Y
STA $D001

LDA #1          ; White
STA $D027       ; Sprite 0 color

; Point to sprite data at $2000
LDA #128        ; $2000 / 64 = 128
STA $07F8

; Move sprite right in game loop
GAME_LOOP:
    INC $D000       ; Increment X position
    ; Add delay/input here
    JMP GAME_LOOP
```

### Pattern 2: Border Color Flash (Classic Demo)

```assembly
FLASH_LOOP:
    LDX #0
FLASH:
    STX $D020       ; Set border to color X
    INX
    CPX #16         ; 16 colors
    BNE FLASH
    JMP FLASH_LOOP  ; Repeat forever
```

### Pattern 3: Collision Detection

```assembly
; Check if sprite 0 hit sprite 1
LDA $D01E       ; Sprite-sprite collision
AND #%00000011  ; Check sprites 0 and 1
BEQ NO_COLLISION
    ; Collision occurred!
    ; Reading $D01E clears it
NO_COLLISION:
```

### Pattern 4: Simple Raster Bar

```assembly
IRQ_HANDLER:
    LDA $D019
    STA $D019       ; Acknowledge

    LDA #2          ; Red
    STA $D020       ; Set border color

    ; Wait a few raster lines
    LDX #10
WAIT:
    DEX
    BNE WAIT

    LDA #0          ; Black
    STA $D020       ; Restore border

    JMP $EA31
```

---

## Smooth Scrolling Basics

### Horizontal Scroll ($D016)

```assembly
; Scroll left by changing X scroll value
LDA $D016
AND #%11111000  ; Clear scroll bits
ORA SCROLL_X    ; Add scroll value (0-7)
STA $D016

; When SCROLL_X reaches 7, shift screen data left
; and reset to 0
```

### Vertical Scroll ($D011)

```assembly
; Scroll up by changing Y scroll value
LDA $D011
AND #%11111000  ; Clear scroll bits
ORA SCROLL_Y    ; Add scroll value (0-7)
STA $D011

; When SCROLL_Y reaches 7, shift screen data up
; and reset to 0
```

---

## Lesson Design Recommendations

### Beginner Lessons (1-10)

**Focus on:**
- Border/background colors ($D020, $D021)
- Screen memory writes ($0400)
- Color RAM ($D800)
- Simple sprite display (no movement yet)

**Example:**
```assembly
; Lesson 3: Change border color
LDA #6
STA $D020
```

### Intermediate Lessons (11-15)

**Add:**
- Sprite movement
- Sprite colors and positioning
- Multiple sprites
- Simple collision detection
- Custom character sets

**Example:**
```assembly
; Lesson 12: Moving sprite with joystick
LDA $DC00       ; Read joystick
AND #$08        ; Right?
BEQ MOVE_RIGHT
; ... check other directions
MOVE_RIGHT:
    INC $D000   ; Move sprite 0 right
```

### Advanced Lessons (16-20)

**Add:**
- Raster interrupts
- Color cycling effects
- Sprite multiplexing
- Bitmap graphics
- Hardware scrolling

**Example:**
```assembly
; Lesson 18: Color bars with raster IRQ
; Set up multiple IRQs at different raster lines
; Change border color at each line
```

---

## Common Mistakes and Fixes

| Problem | Cause | Fix |
|---------|-------|-----|
| Sprite doesn't appear | Not enabled | Set bit in $D015 |
| Sprite wrong location | Forgot X MSB | Check $D010 for X > 255 |
| Sprite pointer wrong | Wrong calculation | Address / 64 = pointer value |
| Raster IRQ doesn't fire | Didn't acknowledge | Read/write $D019 in handler |
| Wrong colors on screen | Used PETSCII not screen codes | Convert to screen codes |
| Custom charset not showing | Wrong $D018 value | Check VM and CB bits |
| Screen in wrong location | VIC bank not set | Set CIA #2 $DD00 bits 0-1 |

---

## Quick Reference: Common Values

### Colors
```assembly
BLACK = 0
WHITE = 1
RED = 2
CYAN = 3
PURPLE = 4
GREEN = 5
BLUE = 6
YELLOW = 7
```

### Sprite Enable (one sprite per bit)
```assembly
$D015 = %00000001  ; Sprite 0 only
$D015 = %00000011  ; Sprites 0 and 1
$D015 = %11111111  ; All 8 sprites
```

### Memory Setup Examples
```assembly
$D018 = $14  ; Screen $0400, Charset $1000 (default)
$D018 = $18  ; Screen $0400, Charset $2000
$D018 = $1C  ; Screen $0400, Charset $2800
```

---

## See Also

- **VIC-II-REFERENCE.md** - Complete hardware specifications
- **SPRITES-REFERENCE.md** - Detailed sprite programming
- **C64-MEMORY-MAP.md** - Memory layout including VIC-II
- **CIA-QUICK-REFERENCE.md** - Input/timer control
- **PETSCII-SCREEN-CODES.md** - Character code conversion

---

**Document Version:** 1.0
**Synthesized:** 2025 for Code Like It's 198x curriculum
**Focus:** Programming essentials - registers, sprites, colors, effects
