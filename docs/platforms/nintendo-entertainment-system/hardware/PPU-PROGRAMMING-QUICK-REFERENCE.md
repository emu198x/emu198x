# NES PPU Programming Quick Reference

**Purpose:** Fast lookup for PPU registers, nametables, sprites, and graphics programming
**Audience:** NES assembly programmers and curriculum designers
**For comprehensive PPU details:** See NESDev Wiki PPU documentation

---

## PPU Overview

The **Picture Processing Unit (PPU)** is the NES's dedicated graphics chip:
- Independent from CPU
- 256×240 resolution (NTSC)
- 60 Hz refresh rate
- Tile-based rendering (8×8 pixel tiles)
- 64 hardware sprites
- 4 background palettes + 4 sprite palettes

**Key Concept:** CPU writes to PPU registers during VBlank. PPU renders independently.

---

## PPU Registers (Memory-Mapped)

### $2000: PPUCTRL (PPU Control Register)

**Write-only**

```
Bit 7: NMI Enable (0=off, 1=generate NMI at start of VBlank)
Bit 6: PPU Master/Slave (ignore - for playchoice, unused)
Bit 5: Sprite Size (0=8×8, 1=8×16)
Bit 4: Background Pattern Table (0=$0000, 1=$1000)
Bit 3: Sprite Pattern Table (0=$0000, 1=$1000; ignored in 8×16 mode)
Bit 2: VRAM Increment (0=add 1 going across, 1=add 32 going down)
Bit 1-0: Base Nametable Address (0=$2000, 1=$2400, 2=$2800, 3=$2C00)
```

**Common Values:**
```asm
%10000000  ; NMI on, 8×8 sprites, increment by 1, nametable $2000
%10100000  ; NMI on, 8×16 sprites, increment by 1, nametable $2000
%00000000  ; Everything off (init/reset)
```

**Example:**
```asm
LDA #%10100000  ; Enable NMI, 8×16 sprites, pattern table 1
STA $2000
```

### $2001: PPUMASK (PPU Mask Register)

**Write-only**

```
Bit 7: Emphasize Blue (NTSC) / Green (PAL)
Bit 6: Emphasize Green (NTSC) / Red (PAL)
Bit 5: Emphasize Red (NTSC) / Blue (PAL)
Bit 4: Show Sprites (1=show, 0=hide)
Bit 3: Show Background (1=show, 0=hide)
Bit 2: Show Sprites in Leftmost 8 Pixels
Bit 1: Show Background in Leftmost 8 Pixels
Bit 0: Greyscale (1=greyscale, 0=color)
```

**Common Values:**
```asm
%00011110  ; Show sprites and background, normal color
%00011010  ; Show background only
%00010100  ; Show sprites only (rare)
%00000000  ; Hide everything (init/reset)
```

**Example:**
```asm
LDA #%00011110  ; Enable rendering
STA $2001
```

### $2002: PPUSTATUS (PPU Status Register)

**Read-only** (reading resets several internal counters)

```
Bit 7: VBlank Flag (1=in VBlank, cleared on read)
Bit 6: Sprite 0 Hit (1=sprite 0 overlaps background)
Bit 5: Sprite Overflow (1=more than 8 sprites on scanline)
Bit 4-0: Open bus (return last value on PPU bus)
```

**Reading $2002:**
- Clears VBlank flag (bit 7)
- Resets PPUADDR/PPUSCROLL write latch

**Common Patterns:**
```asm
; Wait for VBlank
:   BIT $2002       ; Check PPUSTATUS
    BPL :-          ; Loop while bit 7 = 0

; Reset address latch before PPUADDR writes
LDA $2002           ; Read (resets latch)
LDA #$3F
STA $2006           ; First write (high byte)
LDA #$00
STA $2006           ; Second write (low byte)
```

### $2003: OAMADDR (OAM Address Register)

**Write-only**

Sets the address in OAM (sprite memory) for $2004 reads/writes.

**Common Usage:** Usually set to $00, then use $4014 DMA instead of $2004.

```asm
LDA #$00
STA $2003           ; Set OAM address to 0
```

### $2004: OAMDATA (OAM Data Register)

**Read/Write**

Read or write one byte of OAM data. Address auto-increments.

**Warning:** Very slow (1 byte per write). Use $4014 DMA instead during VBlank.

**Rarely used directly:** OAM DMA ($4014) is the standard method.

### $2005: PPUSCROLL (PPU Scroll Register)

**Write-only (write twice)**

Sets scroll position for background rendering.

```
First write:  X scroll (0-255)
Second write: Y scroll (0-239)
```

**Order matters:**
1. Read $2002 (reset latch)
2. Write X scroll to $2005
3. Write Y scroll to $2005

**Example:**
```asm
LDA $2002           ; Reset latch
LDA #0
STA $2005           ; X scroll = 0
STA $2005           ; Y scroll = 0
```

**Pong (no scrolling):**
```asm
; Always set to 0,0 after drawing
LDA $2002
LDA #0
STA $2005
STA $2005
```

### $2006: PPUADDR (PPU Address Register)

**Write-only (write twice)**

Sets PPU memory address for $2007 reads/writes.

```
First write:  High byte of address
Second write: Low byte of address
```

**Must write twice:** 16-bit address in two 8-bit writes.

**Auto-increment:** Address increments after each $2007 access (by 1 or 32, controlled by $2000 bit 2).

**Example:**
```asm
LDA $2002           ; Reset latch
LDA #$3F            ; High byte
STA $2006
LDA #$00            ; Low byte
STA $2006
; Ready to write to $3F00 via $2007
```

### $2007: PPUDATA (PPU Data Register)

**Read/Write**

Read or write PPU memory at current PPUADDR. Address auto-increments.

**Writes:**
```asm
; Write palette
LDA $2002
LDA #$3F
STA $2006
LDA #$00
STA $2006
LDA #$29            ; Color value
STA $2007           ; Write to $3F00, addr now $3F01
```

**Reads (with delay):**
```asm
; Read has 1-byte delay (buffer)
LDA $2002
LDA #$20
STA $2006
LDA #$00
STA $2006
LDA $2007           ; Dummy read (fills buffer)
LDA $2007           ; Actual data from $2000
```

---

## $4014: OAMDMA (OAM DMA Register)

**Write-only** (technically APU/IO register, not PPU)

Triggers DMA copy of 256 bytes from CPU RAM to OAM.

**Write:** High byte of source address (e.g., $02 for $0200-$02FF)

**Usage:**
```asm
LDA #$02            ; High byte of $0200
STA $4014           ; Copy $0200-$02FF → OAM (513 cycles)
```

**Critical:**
- Takes 513-514 CPU cycles
- Must happen during VBlank
- Suspends CPU while copying
- Standard method for sprite updates

---

## PPU Memory Map

```
$0000-$0FFF: Pattern Table 0 (256 tiles, 4KB)
$1000-$1FFF: Pattern Table 1 (256 tiles, 4KB)
$2000-$23BF: Nametable 0 (960 bytes, 32×30 tiles)
$23C0-$23FF: Attribute Table 0 (64 bytes)
$2400-$27FF: Nametable 1
$2800-$2BFF: Nametable 2
$2C00-$2FFF: Nametable 3
$3000-$3EFF: Mirrors of $2000-$2EFF
$3F00-$3F0F: Background Palettes (16 bytes)
$3F10-$3F1F: Sprite Palettes (16 bytes)
$3F20-$3FFF: Mirrors of $3F00-$3F1F
```

### Pattern Tables (CHR-ROM)

**Location:** $0000-$1FFF (8KB total)
**Format:** 256 tiles per table, 16 bytes per tile
**Structure:** Two 8-byte planes (2 bits per pixel = 4 colors)

**Tile Format:**
```
Bytes 0-7:   Plane 0 (low bit of each pixel)
Bytes 8-15:  Plane 1 (high bit of each pixel)

Pixel value = (plane1_bit << 1) | plane0_bit
Result: 0-3 (indexes into palette)
```

**Example Tile (solid square):**
```asm
.segment "CHARS"
.byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF  ; Plane 0 (all 1s)
.byte $FF,$FF,$FF,$FF,$FF,$FF,$FF,$FF  ; Plane 1 (all 1s)
; All pixels = 3 (brightest color in palette)
```

### Nametables (Background Layout)

**Location:** $2000-$2FFF (4 nametables, hardware has 2KB VRAM)
**Size:** 960 bytes per nametable (32×30 tiles)
**Mirroring:** Horizontal or vertical (set by cartridge/iNES header)

**Address Calculation:**
```
address = $2000 + (row × 32) + column
```

**Example (top-left corner):**
```asm
LDA $2002
LDA #$20            ; Nametable 0 ($2000)
STA $2006
LDA #$00            ; Row 0, column 0
STA $2006
LDA #$01            ; Tile $01 (solid block)
STA $2007
```

**Drawing a Row:**
```asm
LDA $2002
LDA #$20            ; Row 0
STA $2006
LDA #$00
STA $2006

LDX #32             ; 32 tiles across
LDA #$01            ; Tile to draw
:   STA $2007       ; Write tile (auto-increment)
    DEX
    BNE :-
```

### Attribute Tables (Palette Selection)

**Location:** $23C0-$23FF (follows each nametable)
**Size:** 64 bytes (8×8 grid, each byte covers 4×4 tiles = 16×16 pixels)
**Format:** Each byte defines palette for 2×2 groups of 2×2 tiles

```
Each byte:
Bits 7-6: Bottom-right 2×2 tiles
Bits 5-4: Bottom-left 2×2 tiles
Bits 3-2: Top-right 2×2 tiles
Bits 1-0: Top-left 2×2 tiles

Values: 00=palette 0, 01=palette 1, 10=palette 2, 11=palette 3
```

**Example (all palette 0):**
```asm
LDA $2002
LDA #$23
STA $2006
LDA #$C0            ; Attribute table start
STA $2006

LDA #%00000000      ; All palette 0
LDX #64
:   STA $2007
    DEX
    BNE :-
```

### Palettes

**Background Palettes:** $3F00-$3F0F
**Sprite Palettes:** $3F10-$3F1F

**Structure:**
```
$3F00: Universal background color (shown everywhere behind tiles/sprites)
$3F01-$3F03: Background palette 0, colors 1-3
$3F04: Mirror of $3F00
$3F05-$3F07: Background palette 1, colors 1-3
$3F08: Mirror of $3F00
$3F09-$3F0B: Background palette 2, colors 1-3
$3F0C: Mirror of $3F00
$3F0D-$3F0F: Background palette 3, colors 1-3

$3F10: Unused (usually mirror of $3F00)
$3F11-$3F13: Sprite palette 0, colors 1-3
$3F14: Unused
$3F15-$3F17: Sprite palette 1, colors 1-3
$3F18: Unused
$3F19-$3F1B: Sprite palette 2, colors 1-3
$3F1C: Unused
$3F1D-$3F1F: Sprite palette 3, colors 1-3
```

**Color 0 in each palette:**
- Background: Universal background ($3F00)
- Sprites: Transparent (shows background through)

**Loading Palettes:**
```asm
LDA $2002
LDA #$3F
STA $2006
LDA #$00
STA $2006

; Background palette 0
LDA #$0F            ; Black (universal background)
STA $2007
LDA #$30            ; White
STA $2007
LDA #$30            ; White
STA $2007
LDA #$30            ; White
STA $2007

; Fill remaining palettes
; ...
```

### NES Color Palette (64 colors)

**Common Colors:**
```
$0F: Black
$00: Dark grey
$10: Light grey
$20/$30: White (same)
$02/$12/$22: Various blues
$06/$16/$26/$36: Various reds
$0A/$1A/$2A: Various greens
$04/$14/$24: Various purples
$08/$18/$28: Various browns/oranges
```

**Note:** Colors look different on actual hardware vs emulators. Test on target.

---

## Sprite System (OAM)

**Hardware:** 64 sprites, 8×8 or 8×16 pixels each
**OAM:** 256 bytes inside PPU ($00-$FF)
**CPU Access:** Via OAM buffer ($0200-$02FF) + DMA

### OAM Format (4 bytes per sprite)

```
Byte 0: Y Position (0-239, $EF hides sprite)
Byte 1: Tile Index (0-255)
Byte 2: Attributes
Byte 3: X Position (0-255)
```

**Sprite Attributes (Byte 2):**
```
Bit 7: Vertical Flip (1=flip)
Bit 6: Horizontal Flip (1=flip)
Bit 5: Priority (0=in front of background, 1=behind background)
Bit 4-3: Unused
Bit 2: Unused (PPU version specific)
Bit 1-0: Palette (0-3, selects sprite palette)
```

**OAM Buffer Example:**
```asm
; Sprite 0 at ($0200-$0203)
LDA #100
STA $0200           ; Y position
LDA #$00
STA $0201           ; Tile $00
LDA #%00000000
STA $0202           ; Palette 0, no flip, in front
LDA #50
STA $0203           ; X position
```

**Hiding Sprites:**
```asm
; Set Y = $FF (off-screen)
LDX #$04            ; Start at sprite 1
LDA #$FF
:   STA $0200,X     ; Hide sprite (Y position = $FF)
    INX
    INX
    INX
    INX
    BNE :-          ; Loop through all 64 sprites
```

### Sprite Tile Selection (8×16 mode)

When PPUCTRL bit 5 = 1 (8×16 sprites):
- Tile index must be EVEN (bit 0 ignored)
- Top half: tile N
- Bottom half: tile N+1
- Pattern table selected by bit 0 of tile index:
  - Bit 0 = 0: Pattern table 0 ($0000)
  - Bit 0 = 1: Pattern table 1 ($1000)

**Example:**
```asm
LDA #$00            ; Top half = tile $00, bottom = $01
STA $0201           ; Uses pattern table 0

LDA #$02            ; Top half = tile $02, bottom = $03
STA $0201           ; Uses pattern table 0
```

---

## VBlank Timing

**NTSC:** 60 Hz, 262 scanlines per frame
- Visible: Scanlines 0-239 (256×240 pixels)
- VBlank: Scanlines 241-260 (20 scanlines)
- Pre-render: Scanline 261

**VBlank Window:** ~2273 CPU cycles (20 scanlines × 113.66 cycles/scanline)

**Available Time:**
- OAM DMA: 513 cycles
- Remaining: ~1760 cycles for palette/nametable updates

**Typical VBlank Usage:**
```asm
NMI:
    PHA             ; 3 cycles
    TXA
    PHA             ; 6 cycles total
    TYA
    PHA             ; 9 cycles total

    LDA #$02
    STA $4014       ; OAM DMA (513 cycles)

    ; Palette updates (~50-100 cycles)
    ; Scroll updates (~10 cycles)
    ; Nametable changes (7 cycles per byte)

    PLA
    TAY
    PLA
    TAX
    PLA
    RTI             ; Return (6 cycles)
```

---

## Common PPU Patterns

### PPU Initialization (Reset)
```asm
Reset:
    SEI             ; Disable interrupts
    CLD             ; Clear decimal (no effect but good practice)

    ; Wait for first VBlank
:   BIT $2002
    BPL :-

    ; Initialize RAM, zero page, etc.

    ; Wait for second VBlank (PPU stable)
:   BIT $2002
    BPL :-

    ; Load palettes, nametable, etc.

    ; Enable NMI and rendering
    LDA #%10000000
    STA $2000
    LDA #%00011110
    STA $2001
```

### Writing to Nametable
```asm
; Always reset latch first
LDA $2002

; Set address
LDA #$20            ; High byte
STA $2006
LDA #$42            ; Low byte ($2042 = row 2, column 2)
STA $2006

; Write data
LDA #$05            ; Tile $05
STA $2007           ; Write (auto-increments to $2043)
```

### Clearing Nametable
```asm
LDA $2002
LDA #$20
STA $2006
LDA #$00
STA $2006

LDA #$00            ; Blank tile
LDX #$04            ; 4 × 256 = 1024 bytes
:   LDY #$00
:   STA $2007
    DEY
    BNE :-
    DEX
    BNE :--
```

### Safe PPU Writes (VBlank Only)
```asm
MainLoop:
    ; Wait for NMI flag
:   LDA nmi_ready
    BEQ :-

    ; Clear flag
    LDA #$00
    STA nmi_ready

    ; Prepare data (outside VBlank is safe)
    JSR PrepareSprites

    JMP MainLoop

NMI:
    ; Save registers
    PHA
    TXA
    PHA
    TYA
    PHA

    ; OAM DMA
    LDA #$02
    STA $4014

    ; Set flag
    LDA #$01
    STA nmi_ready

    ; Restore registers
    PLA
    TAY
    PLA
    TAX
    PLA
    RTI
```

---

## PPU Limitations & Gotchas

### 8 Sprites Per Scanline
Hardware limit: 8 sprites visible on any horizontal line.
- 9th sprite causes sprite overflow (PPUSTATUS bit 5)
- Later sprites don't render
- **Solution:** Sprite multiplexing, careful placement

### Attribute Table Granularity
Palette selected per 16×16 pixel area (4×4 tiles).
- Can't have different palettes for adjacent 8×8 tiles
- **Workaround:** Use sprites for fine-grained color

### No Mid-Frame PPU Changes (without advanced techniques)
Standard NMI method: can only update during VBlank.
- **Advanced:** Sprite 0 hit, mapper IRQs (later lessons)

### VRAM Address Corruption
Reading $2007 during rendering glitches display.
- **Rule:** Only access VRAM during VBlank or when rendering disabled

### Palette $X0 Special Cases
- $3F00: Universal background (shows everywhere)
- Color 0 in sprite palettes: Transparent
- $3F10: Often mirrors $3F00

---

## Quick Lookup: Register Usage

| Register | Read | Write | Common Use |
|----------|------|-------|------------|
| $2000 | No | Yes | Enable NMI, set sprite size, nametable |
| $2001 | No | Yes | Enable rendering (sprites/background) |
| $2002 | Yes | No | Check VBlank, reset latch |
| $2003 | No | Yes | Set OAM address (rarely used) |
| $2004 | Yes | Yes | Read/write OAM (use DMA instead) |
| $2005 | No | Yes (2×) | Set scroll position |
| $2006 | No | Yes (2×) | Set VRAM address |
| $2007 | Yes | Yes | Read/write VRAM |
| $4014 | No | Yes | Trigger OAM DMA |

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** NES Phase 1 Assembly Programming

**See Also:**
- 6502-QUICK-REFERENCE.md (CPU instructions)
- NES-MEMORY-MAP.md (complete memory layout)
- CONTROLLER-INPUT-QUICK-REFERENCE.md (reading controllers)

**Next:** Create NES Memory Map and Controller Input references
