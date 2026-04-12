# NES Memory Map Quick Reference

**Purpose:** Fast lookup for NES CPU and PPU memory organization
**Audience:** NES assembly programmers and curriculum designers
**For comprehensive details:** See NESDev Wiki Memory Map documentation

---

## CPU Memory Map ($0000-$FFFF)

The NES CPU (6502) has a 16-bit address space (64KB). Memory is mapped to RAM, PPU registers, APU registers, cartridge ROM, and I/O.

```
$0000-$07FF: Internal RAM (2KB)
$0800-$1FFF: Mirrors of $0000-$07FF (3 times)
$2000-$2007: PPU Registers
$2008-$3FFF: Mirrors of $2000-$2007 (1023 times)
$4000-$4017: APU and I/O Registers
$4018-$401F: APU and I/O (disabled, often open bus)
$4020-$FFFF: Cartridge space (PRG-ROM, PRG-RAM, mapper registers)
```

---

## Internal RAM ($0000-$07FF)

**Size:** 2KB (2048 bytes)
**Mirrored:** $0800-$1FFF (addresses wrap around)

### Zero Page ($0000-$00FF)

**Special:** Fastest memory access (2 cycles vs 3-4 for absolute)
**Use for:** Hot variables, frequently accessed data, temporary storage

**Common Allocation:**
```
$0000-$000F: Temp variables, function parameters
$0010-$00FF: Game variables (player state, counters, etc.)
```

**Example:**
```asm
.segment "ZEROPAGE"
temp:       .res 1      ; $0000
paddle_y:   .res 1      ; $0001
paddle_x:   .res 1      ; $0002
ball_x:     .res 1      ; $0003
ball_y:     .res 1      ; $0004
ball_vx:    .res 1      ; $0005 (signed velocity)
ball_vy:    .res 1      ; $0006 (signed velocity)
buttons:    .res 1      ; $0007
score_p1:   .res 1      ; $0008
score_p2:   .res 1      ; $0009
```

### Stack ($0100-$01FF)

**Size:** 256 bytes
**Usage:** Hardware stack (JSR, RTS, PHA, PLA, interrupts)
**Stack Pointer:** Starts at $FF (points to $01FF), grows downward

**Important:**
- JSR pushes 2 bytes (return address)
- Interrupts (NMI, IRQ) push 3 bytes (PC high, PC low, status)
- PHA/PLA push/pull 1 byte

**Depth Calculation:**
```
Main routine calls:      2 bytes
  Subroutine 1:          2 bytes
    Subroutine 2:        2 bytes
      NMI interrupt:     3 bytes
        Saves A,X,Y:     3 bytes (PHA × 3)
Total: 12 bytes used
```

**Warning:** Deep nesting can overflow stack. Monitor stack usage.

**Don't use $0100-$01FF for variables!**

### OAM Buffer ($0200-$02FF)

**Size:** 256 bytes
**Purpose:** CPU-side buffer for sprite data
**Usage:** Build sprite data here, copy to PPU via DMA ($4014)

**Format:** 64 sprites × 4 bytes each
```
$0200-$0203: Sprite 0 (Y, tile, attr, X)
$0204-$0207: Sprite 1
$0208-$020B: Sprite 2
...
$02FC-$02FF: Sprite 63
```

**Example:**
```asm
; Sprite 0: Paddle at (50, 100)
LDA #100
STA $0200       ; Y position
LDA #$00
STA $0201       ; Tile index
LDA #%00000000
STA $0202       ; Attributes
LDA #50
STA $0203       ; X position
```

### General RAM ($0300-$07FF)

**Size:** 1280 bytes
**Usage:** Game variables, arrays, buffers, temporary data

**Common Allocation:**
```
$0300-$03FF: General variables (256 bytes)
$0400-$04FF: Arrays, tables (256 bytes)
$0500-$05FF: Level data, strings (256 bytes)
$0600-$06FF: Music/sound data (256 bytes)
$0700-$07FF: Buffer/scratch space (256 bytes)
```

**Example:**
```asm
.segment "BSS"          ; Uninitialized data
nmi_ready:    .res 1    ; $0300
frame_count:  .res 1    ; $0301
game_state:   .res 1    ; $0302

enemy_x:      .res 8    ; $0303-$030A (8 enemies)
enemy_y:      .res 8    ; $030B-$0312
enemy_active: .res 8    ; $0313-$031A

level_data:   .res 256  ; $031B-$041A (or wherever)
```

**Typical Pong RAM Usage:**
```asm
.segment "ZEROPAGE"
paddle1_y:  .res 1
paddle2_y:  .res 1
ball_x:     .res 1
ball_y:     .res 1
ball_vx:    .res 1
ball_vy:    .res 1
buttons1:   .res 1
buttons2:   .res 1

.segment "BSS"
score_p1:   .res 1      ; $0300
score_p2:   .res 1      ; $0301
nmi_ready:  .res 1      ; $0302
frame_count: .res 1     ; $0303
```

---

## PPU Registers ($2000-$2007)

**Mirrored:** Every 8 bytes through $3FFF

| Address | Register | Access | Purpose |
|---------|----------|--------|---------|
| $2000 | PPUCTRL | Write | PPU control (NMI, sprites, nametable) |
| $2001 | PPUMASK | Write | Enable rendering, color effects |
| $2002 | PPUSTATUS | Read | VBlank status, sprite 0 hit |
| $2003 | OAMADDR | Write | OAM address (rarely used) |
| $2004 | OAMDATA | R/W | OAM data (use DMA instead) |
| $2005 | PPUSCROLL | Write (2×) | Scroll position |
| $2006 | PPUADDR | Write (2×) | VRAM address |
| $2007 | PPUDATA | R/W | VRAM data |

**See:** PPU-PROGRAMMING-QUICK-REFERENCE.md for detailed register usage

---

## APU and I/O Registers ($4000-$4017)

### Sound Registers ($4000-$4013)

| Address | Register | Purpose |
|---------|----------|---------|
| $4000-$4003 | Pulse 1 | Square wave channel 1 |
| $4004-$4007 | Pulse 2 | Square wave channel 2 |
| $4008-$400B | Triangle | Triangle wave channel |
| $400C-$400F | Noise | Noise channel |
| $4010-$4013 | DMC | Delta modulation channel (samples) |

**Note:** Sound programming is advanced (Tier 7+ in curriculum)

### I/O Registers

| Address | Register | Access | Purpose |
|---------|----------|--------|---------|
| $4014 | OAMDMA | Write | OAM DMA trigger (high byte of source) |
| $4015 | SND_CHN | R/W | Sound channel enable/status |
| $4016 | JOY1 | R/W | Controller 1 data and strobe |
| $4017 | JOY2 | R/W | Controller 2 data and frame counter |

**Controller Usage:**
```asm
; Write $01 then $00 to $4016 to latch buttons
LDA #$01
STA $4016
LDA #$00
STA $4016

; Read 8 times from $4016 for controller 1
; Read 8 times from $4017 for controller 2
```

---

## Cartridge Space ($4020-$FFFF)

**Depends on mapper and cartridge configuration.**

### Typical NROM Mapper (Mapper 0)

**NROM-128 (16KB PRG-ROM):**
```
$8000-$BFFF: PRG-ROM (16KB)
$C000-$FFFF: Mirror of $8000-$BFFF
```

**NROM-256 (32KB PRG-ROM):**
```
$8000-$FFFF: PRG-ROM (32KB, no mirroring)
```

**Vectors (end of PRG-ROM):**
```
$FFFA-$FFFB: NMI vector (address of NMI handler)
$FFFC-$FFFD: Reset vector (address of Reset handler)
$FFFE-$FFFF: IRQ/BRK vector (address of IRQ handler)
```

### PRG-RAM ($6000-$7FFF)

**Optional:** 8KB of battery-backed save RAM (if cartridge has it)
**Usage:** Save games, high scores, persistent data
**Access:** Read/write like normal RAM

**Pong (NROM) doesn't use PRG-RAM.**

---

## PPU Memory Map ($0000-$3FFF)

**Separate address space** accessed via PPU registers ($2006/$2007).

```
$0000-$0FFF: Pattern Table 0 (256 tiles, 4KB, CHR-ROM/RAM)
$1000-$1FFF: Pattern Table 1 (256 tiles, 4KB, CHR-ROM/RAM)
$2000-$23BF: Nametable 0 (960 bytes, background layout)
$23C0-$23FF: Attribute Table 0 (64 bytes, palette selection)
$2400-$27BF: Nametable 1
$27C0-$27FF: Attribute Table 1
$2800-$2BBF: Nametable 2
$2BC0-$2BFF: Attribute Table 2
$2C00-$2FBF: Nametable 3
$2FC0-$2FFF: Attribute Table 3
$3000-$3EFF: Mirrors of $2000-$2EFF
$3F00-$3F0F: Background Palettes (16 bytes)
$3F10-$3F1F: Sprite Palettes (16 bytes)
$3F20-$3FFF: Mirrors of $3F00-$3F1F
```

**Note:** Actual nametable layout depends on mirroring mode (horizontal/vertical/4-screen).

**See:** PPU-PROGRAMMING-QUICK-REFERENCE.md for PPU memory details

---

## Memory Access Patterns

### Fast Access (Zero Page)

**Cycles:** 3 for load, 3 for store
**Use when:** Variable accessed frequently (every frame)

```asm
LDA paddle_y        ; 3 cycles (if paddle_y is in zero page)
CLC
ADC #2
STA paddle_y        ; 3 cycles
```

### Absolute Addressing

**Cycles:** 4 for load, 4 for store
**Use when:** Variable accessed occasionally or outside zero page

```asm
LDA $0300           ; 4 cycles
CLC
ADC #1
STA $0300           ; 4 cycles
```

### Indexed Addressing

**Cycles:** 4-5 (zero page,X), 4-5 (absolute,X)
**Use when:** Arrays, tables, multiple entities

```asm
; Access enemy_x[3]
LDX #3
LDA enemy_x,X       ; 4 cycles (zero page)
```

---

## Common Memory Layouts

### Minimal Pong (Tier 1)

```
Zero Page:
$00-$09: Game variables (10 bytes)

Stack:
$0100-$01FF: Hardware stack (don't touch)

OAM Buffer:
$0200-$020F: 4 sprites (paddle 1, paddle 2, ball, score digits)
$0210-$02FF: Unused (hidden with Y=$FF)

General RAM:
$0300-$0303: Frame counter, NMI flag, temp vars

PRG-ROM:
$8000-$9FFF: Game code (8KB)
$A000-$BFFF: Empty (mirrored if NROM-128)
$C000-$FFFF: Mirror of $8000-$BFFF (NROM-128)

CHR-ROM:
$0000-$0FFF: Pattern table 0 (background tiles)
$1000-$1FFF: Pattern table 1 (sprite tiles)
```

### Typical Action Game (Later Tiers)

```
Zero Page:
$00-$1F: Player variables (position, velocity, state)
$20-$3F: Enemy array pointers
$40-$5F: Collision, physics temps
$60-$7F: Level data pointers
$80-$9F: Sound/music state
$A0-$FF: Misc game state

OAM Buffer:
$0200-$02FF: All 64 sprites (256 bytes)

General RAM:
$0300-$03FF: Enemy state (positions, types, health)
$0400-$04FF: Projectile state
$0500-$05FF: Level/tile data
$0600-$06FF: Music/sound data
$0700-$07FF: Misc buffers

PRG-ROM:
$8000-$9FFF: Bank 0 (switchable, if using mapper)
$A000-$BFFF: Bank 1 (switchable, if using mapper)
$C000-$FFFF: Fixed bank (always present, vectors here)
```

---

## Safe Memory Regions

### Always Safe to Use

- **$0000-$00FF:** Zero page (256 bytes)
  - Fast access
  - Use for hot variables

- **$0200-$02FF:** OAM buffer (256 bytes)
  - Reserved for sprites
  - Don't use for other data

- **$0300-$07FF:** General RAM (1280 bytes)
  - Safe for any purpose
  - Game state, arrays, buffers

### Never Use

- **$0100-$01FF:** Stack
  - Hardware managed
  - Corrupting this crashes program

### Special Purpose

- **$0800-$1FFF:** RAM mirrors
  - Same as $0000-$07FF
  - No benefit to using (wastes address space)

---

## Memory Optimization Tips

### 1. Zero Page is Precious

**Good:**
```asm
; Frequently accessed (every frame)
.segment "ZEROPAGE"
paddle_y: .res 1
ball_x:   .res 1
ball_y:   .res 1
```

**Bad:**
```asm
; Rarely accessed (once per second)
.segment "ZEROPAGE"
high_score: .res 2      ; Wastes zero page space
```

### 2. Pack Boolean Flags

**Good:**
```asm
; 8 flags in 1 byte
game_flags: .res 1
; Bit 0: Game over
; Bit 1: Level complete
; Bit 2: Sound enabled
; etc.

LDA game_flags
AND #%00000001      ; Test game over
BNE game_over
```

**Bad:**
```asm
; 8 flags in 8 bytes
game_over:      .res 1
level_complete: .res 1
sound_enabled:  .res 1
; ... (wastes 7 bytes)
```

### 3. Reuse Temporary Variables

**Good:**
```asm
.segment "ZEROPAGE"
temp: .res 1            ; Reused everywhere

UpdatePlayer:
    LDA player_x
    STA temp
    ; Use temp
    RTS

UpdateEnemy:
    LDA enemy_x
    STA temp
    ; Reuse temp (not called simultaneously)
    RTS
```

### 4. Use Absolute for Cold Data

**Good:**
```asm
.segment "ZEROPAGE"
frame_count: .res 1     ; Updated every frame

.segment "BSS"
high_score:  .res 2     ; Updated rarely (absolute address)
```

---

## Debugging Memory Issues

### Stack Overflow

**Symptoms:**
- Random crashes
- Variables corrupted
- Return addresses wrong

**Check:**
```asm
; At strategic points
TSX                 ; Transfer SP to X
CPX #$F0            ; Check if SP < $F0 (danger zone)
BCC stack_warning   ; Branch if stack too deep
```

### RAM Corruption

**Symptoms:**
- Variables change unexpectedly
- Sprites flicker
- Game state inconsistent

**Check:**
- Are you writing past array bounds?
- Are interrupts modifying shared variables?
- Is stack overflowing into $0200+?

### OAM Buffer Issues

**Symptoms:**
- Sprites in wrong positions
- Sprites appear/disappear randomly

**Check:**
- Did you hide unused sprites (Y=$FF)?
- Are you writing past $02FF?
- Is DMA address correct ($02)?

---

## Quick Lookup Tables

### Memory Region Summary

| Range | Size | Purpose | Speed |
|-------|------|---------|-------|
| $0000-$00FF | 256B | Zero page | Fast (3 cycles) |
| $0100-$01FF | 256B | Stack | Don't touch |
| $0200-$02FF | 256B | OAM buffer | Normal (4 cycles) |
| $0300-$07FF | 1280B | General RAM | Normal (4 cycles) |
| $2000-$2007 | 8B | PPU registers | Varies |
| $4000-$4017 | 24B | APU/I/O | Varies |
| $8000-$FFFF | 32KB | PRG-ROM | Read-only |

### Vector Table ($FFFA-$FFFF)

| Address | Vector | Purpose |
|---------|--------|---------|
| $FFFA-$FFFB | NMI | Called at start of VBlank (60 Hz) |
| $FFFC-$FFFD | Reset | Called on power-on/reset |
| $FFFE-$FFFF | IRQ/BRK | Called on IRQ or BRK instruction |

**Example (ca65):**
```asm
.segment "VECTORS"
.word NMI_Handler       ; $FFFA
.word Reset_Handler     ; $FFFC
.word IRQ_Handler       ; $FFFE
```

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** NES Phase 1 Assembly Programming

**See Also:**
- 6502-QUICK-REFERENCE.md (CPU instructions, addressing modes)
- PPU-PROGRAMMING-QUICK-REFERENCE.md (PPU memory map)
- CONTROLLER-INPUT-QUICK-REFERENCE.md (reading controllers)

**Next:** Controller Input Quick Reference
