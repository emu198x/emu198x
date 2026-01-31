# NES / Famicom

## Overview

| Property | Value |
|----------|-------|
| CPU | Ricoh 2A03 (6502 variant) @ 1.79 MHz |
| Crystal | 21.477272 MHz (NTSC) / 26.601712 MHz (PAL) |
| RAM | 2K internal |
| Video | PPU 2C02, 256×240, 54 colours |
| Audio | APU (integrated in 2A03) |
| Release | 1983 (Japan), 1985 (NA) |

## Timing

### Crystal Derivation (NTSC)

```
Crystal: 21.477272 MHz
   ÷4  → 5.369318 MHz (PPU clock)
   ÷12 → 1.789773 MHz (CPU clock)
   
PPU runs at exactly 3× CPU rate.
```

### Crystal Derivation (PAL)

```
Crystal: 26.601712 MHz
   ÷5  → 5.320342 MHz (PPU clock)
   ÷16 → 1.662607 MHz (CPU clock)
   
PPU runs at 3.2× CPU rate.
```

### Frame Timing (NTSC)

| Property | Value |
|----------|-------|
| PPU dots per scanline | 341 |
| Scanlines per frame | 262 |
| PPU cycles per frame | 89342 |
| CPU cycles per frame | 29780.67 |
| Frame rate | 60.0988 Hz |

Note: CPU cycles per frame alternates between 29780 and 29781 (odd/even frames).

### Frame Timing (PAL)

| Property | Value |
|----------|-------|
| PPU dots per scanline | 341 |
| Scanlines per frame | 312 |
| PPU cycles per frame | 106392 |
| CPU cycles per frame | 33247.5 |
| Frame rate | 50.007 Hz |

### Phase Relationship

PPU and CPU alignment matters for accurate sprite 0 hit and VBlank timing.

```
Crystal: |0|1|2|3|4|5|6|7|8|9|10|11|
PPU:     |*|.|.|.|*|.|.|.|*|.|. |. |  (every 4)
CPU:     |*|.|.|.|.|.|.|.|.|.|. |. |  (every 12)
```

Every CPU cycle spans exactly 3 PPU cycles (NTSC).

## Memory Map

### CPU Memory Map

| Address | Size | Contents |
|---------|------|----------|
| $0000-$07FF | 2K | Internal RAM |
| $0800-$1FFF | — | Mirrors of $0000-$07FF |
| $2000-$2007 | 8 | PPU registers |
| $2008-$3FFF | — | Mirrors of $2000-$2007 |
| $4000-$4017 | 24 | APU and I/O registers |
| $4018-$401F | 8 | APU test (disabled) |
| $4020-$FFFF | ~48K | Cartridge space |

### PPU Memory Map

| Address | Size | Contents |
|---------|------|----------|
| $0000-$0FFF | 4K | Pattern table 0 (CHR) |
| $1000-$1FFF | 4K | Pattern table 1 (CHR) |
| $2000-$23FF | 1K | Nametable 0 |
| $2400-$27FF | 1K | Nametable 1 |
| $2800-$2BFF | 1K | Nametable 2 |
| $2C00-$2FFF | 1K | Nametable 3 |
| $3000-$3EFF | — | Mirrors of $2000-$2EFF |
| $3F00-$3F1F | 32 | Palette RAM |
| $3F20-$3FFF | — | Mirrors of $3F00-$3F1F |

Note: NES only has 2K of VRAM. Two nametables are mirrors unless cartridge provides extra VRAM.

## PPU (Picture Processing Unit)

### Registers

| Address | Name | Access | Function |
|---------|------|--------|----------|
| $2000 | PPUCTRL | Write | Control register |
| $2001 | PPUMASK | Write | Mask register |
| $2002 | PPUSTATUS | Read | Status register |
| $2003 | OAMADDR | Write | OAM address |
| $2004 | OAMDATA | R/W | OAM data |
| $2005 | PPUSCROLL | Write ×2 | Scroll position |
| $2006 | PPUADDR | Write ×2 | VRAM address |
| $2007 | PPUDATA | R/W | VRAM data |

### PPUCTRL ($2000)

```
Bit 7: Generate NMI at VBlank
Bit 6: PPU master/slave (0 = master)
Bit 5: Sprite size (0 = 8×8, 1 = 8×16)
Bit 4: Background pattern table
Bit 3: Sprite pattern table
Bit 2: VRAM increment (0 = +1, 1 = +32)
Bits 1-0: Base nametable address
```

### PPUMASK ($2001)

```
Bit 7: Emphasize blue
Bit 6: Emphasize green
Bit 5: Emphasize red
Bit 4: Show sprites
Bit 3: Show background
Bit 2: Show sprites in leftmost 8 pixels
Bit 1: Show background in leftmost 8 pixels
Bit 0: Greyscale
```

### PPUSTATUS ($2002)

```
Bit 7: VBlank flag (cleared on read)
Bit 6: Sprite 0 hit
Bit 5: Sprite overflow
```

### Rendering

Each scanline:
1. Cycles 0: Idle
2. Cycles 1-256: Render pixels (fetch tiles, sprites)
3. Cycles 257-320: Sprite evaluation for next line
4. Cycles 321-336: Prefetch first two tiles
5. Cycles 337-340: Unused fetches

### Sprite 0 Hit

Set when non-transparent sprite 0 pixel overlaps non-transparent background pixel. Timing is cycle-accurate and used by games for split-screen effects.

### VBlank

- Line 241, cycle 1: VBlank flag set, NMI triggered (if enabled)
- Line 261 (pre-render): VBlank flag cleared at cycle 1

## APU (Audio Processing Unit)

### Channels

| Channel | Type | Registers |
|---------|------|-----------|
| Pulse 1 | Square wave | $4000-$4003 |
| Pulse 2 | Square wave | $4004-$4007 |
| Triangle | Triangle wave | $4008-$400B |
| Noise | Noise | $400C-$400F |
| DMC | Sample playback | $4010-$4013 |

### Pulse Channel Registers

```
$4000/$4004: DDLC VVVV (duty, loop, constant, volume/envelope)
$4001/$4005: EPPP NSSS (sweep enable, period, negate, shift)
$4002/$4006: TTTT TTTT (timer low)
$4003/$4007: LLLL LTTT (length counter load, timer high)
```

### Frame Counter ($4017)

```
Bit 7: Mode (0 = 4-step, 1 = 5-step)
Bit 6: Interrupt inhibit
```

Controls envelope, length counter, and sweep timing.

## Controller

### Register $4016 (Write)

Strobe bit (bit 0). Write 1 then 0 to latch controller state.

### Register $4016/$4017 (Read)

Shift out button states. Standard controller returns:

A, B, Select, Start, Up, Down, Left, Right (then 1s forever).

## Mappers

Mappers handle bank switching for PRG ROM, CHR ROM/RAM, and provide additional features.

### Mapper 0 (NROM)

No banking. Up to 32K PRG, 8K CHR.

### Mapper 1 (MMC1)

```
PRG banking: 16K or 32K modes
CHR banking: 4K or 8K modes
Mirroring: Switchable
```

Serial register: write bit 0 five times, then value is latched.

### Mapper 2 (UxROM)

```
PRG: 16K banks, switchable at $8000
CHR: 8K RAM
```

### Mapper 3 (CNROM)

```
PRG: Fixed 16K/32K
CHR: 8K banks, switchable
```

### Mapper 4 (MMC3)

```
PRG: 8K banks
CHR: 2K/1K banks
Scanline counter: IRQ at configurable line
```

The scanline counter IRQ is crucial for many games' effects.

## Media Formats

### iNES Format (.nes)

```
Header (16 bytes):
  0-3: "NES\x1A"
  4: PRG ROM size (16K units)
  5: CHR ROM size (8K units)
  6: Flags 6 (mapper low, mirroring, battery, trainer)
  7: Flags 7 (mapper high, format)
  8-15: Extended flags or zero

PRG ROM: N × 16K bytes
CHR ROM: M × 8K bytes (or 0 if CHR RAM)
```

### NES 2.0 Format

Extended iNES with more mapper bits, submapper, PRG/CHR RAM sizes, timing region.

### FDS Format

Famicom Disk System. 65500 bytes per side.

## Verification Files

```
# Test ROMs (blargg)
cpu_test/official_only.nes
cpu_test/all_instrs.nes
ppu_vbl_nmi/ppu_vbl_nmi.nes
apu_test/apu_test.nes
sprite_hit_tests_2005.10.05/sprite_hit_tests.nes

# Games
Nintendo NES/Games/Super Mario Bros. (1985)(Nintendo).nes [Mapper 0]
Nintendo NES/Games/Legend of Zelda, The (1986)(Nintendo).nes [Mapper 1]
Nintendo NES/Games/Mega Man (1987)(Capcom).nes [Mapper 2]
Nintendo NES/Games/Super Mario Bros. 3 (1988)(Nintendo).nes [Mapper 4]
```

## Common Pitfalls

1. **PPU/CPU alignment** — Many games depend on exact cycle relationships.
2. **Sprite 0 hit timing** — Frame early or late breaks split-screen.
3. **Mapper IRQ** — MMC3 scanline counter is notoriously fiddly.
4. **Open bus** — Reads from unmapped addresses return last bus value.
5. **PPUDATA read buffer** — Reads are delayed by one fetch.
6. **Odd frame cycle skip** — NTSC skips one PPU cycle on odd frames with rendering enabled.
