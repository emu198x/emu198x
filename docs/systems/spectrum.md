# ZX Spectrum

## Overview

| Property | Value |
|----------|-------|
| CPU | Zilog Z80A @ 3.5 MHz |
| Crystal | 14.000 MHz (14.112 MHz on some) |
| RAM | 16K or 48K (128K on later models) |
| Video | ULA, 256×192, 15 colours |
| Audio | 1-bit beeper (AY-3-8912 on 128K) |
| Release | 1982 |

## Timing

### Crystal Derivation

```
Crystal: 14.000 MHz
   ÷2 → 7.000 MHz (pixel clock / ULA)
   ÷4 → 3.500 MHz (CPU clock)
```

### Frame Timing (48K PAL)

| Property | Value |
|----------|-------|
| T-states per line | 224 |
| Lines per frame | 312 |
| T-states per frame | 69888 |
| Frame rate | 50.08 Hz |
| CPU clock | 3.5 MHz |

### Scanline Breakdown

| T-states | Event |
|----------|-------|
| 0-127 | Screen area (128 pixels) |
| 128-175 | Right border (48 pixels) |
| 176-207 | Horizontal blank (32 pixels) |
| 208-223 | Left border (16 pixels) |

### Vertical Breakdown

| Lines | Region |
|-------|--------|
| 0-63 | Top border |
| 64-255 | Screen area (192 lines) |
| 256-311 | Bottom border + VBlank |

### Phase Relationship

ULA and CPU both derive from the 14 MHz crystal:
- ULA ticks on crystal cycles 0, 2, 4, 6... (every 2)
- CPU ticks on crystal cycles 0, 4, 8, 12... (every 4)

```
Crystal: |0|1|2|3|4|5|6|7|8|9|A|B|C|D|E|F|
ULA:     |*|.|*|.|*|.|*|.|*|.|*|.|*|.|*|.|
CPU:     |*|.|.|.|*|.|.|.|*|.|.|.|*|.|.|.|
```

## Memory Map

### 48K Spectrum

| Address | Size | Contents |
|---------|------|----------|
| $0000-$3FFF | 16K | ROM |
| $4000-$57FF | 6K | Screen bitmap |
| $5800-$5AFF | 768 | Attributes |
| $5B00-$FFFF | 42K | RAM |

### Screen Memory Layout

Bitmap memory ($4000-$57FF) is notoriously non-linear:

```
Address = $4000 + ((Y & 0xC0) << 5) + ((Y & 0x07) << 8) + ((Y & 0x38) << 2) + X
```

Or in thirds:
- Lines 0-63: $4000-$47FF
- Lines 64-127: $4800-$4FFF
- Lines 128-191: $5000-$57FF

Within each third, lines interleave by 8.

### Attribute Memory

Each attribute byte covers an 8×8 pixel cell:

```
Bit 7: FLASH
Bit 6: BRIGHT
Bits 5-3: PAPER (background)
Bits 2-0: INK (foreground)
```

### 128K Banking

Port $7FFD controls banking:

```
Bits 2-0: RAM bank at $C000 (0-7)
Bit 3: Screen select (0=normal, 1=shadow)
Bit 4: ROM select (0=128K, 1=48K)
Bit 5: Disable paging (lock until reset)
```

## ULA

The ULA handles video generation, keyboard, tape, and beeper.

### I/O Port $FE

**Read (keyboard):**
Address lines A8-A15 select keyboard half-rows.

| A15-A8 | Row | Keys |
|--------|-----|------|
| $FE | 0 | CAPS SHIFT, Z, X, C, V |
| $FD | 1 | A, S, D, F, G |
| $FB | 2 | Q, W, E, R, T |
| $F7 | 3 | 1, 2, 3, 4, 5 |
| $EF | 4 | 0, 9, 8, 7, 6 |
| $DF | 5 | P, O, I, U, Y |
| $BF | 6 | ENTER, L, K, J, H |
| $7F | 7 | SPACE, SYM SHIFT, M, N, B |

**Write:**
```
Bits 2-0: Border colour
Bit 3: MIC output
Bit 4: EAR output (beeper)
```

### Contention

During screen fetch, the ULA needs the bus. CPU is delayed if it accesses $4000-$7FFF during contended cycles.

**Contention pattern (repeating):** 6, 5, 4, 3, 2, 1, 0, 0

This means:
- First contended T-state: wait 6 T-states
- Second: wait 5
- ...
- Seventh: wait 0
- Eighth: wait 0
- Then repeat

**I/O contention:**
Even I/O addresses (bit 0 = 0) are contended at the ULA port regardless of memory address.

## AY-3-8912 (128K)

### Registers

| Register | Function |
|----------|----------|
| 0-1 | Channel A period (12-bit) |
| 2-3 | Channel B period |
| 4-5 | Channel C period |
| 6 | Noise period (5-bit) |
| 7 | Mixer control |
| 8-10 | Channel A/B/C volume (4-bit, or envelope) |
| 11-12 | Envelope period (16-bit) |
| 13 | Envelope shape |

### I/O Ports

```
$FFFD: Register select (active low A1, active high A15)
$BFFD: Data write
```

## Media Formats

### TAP Format

Sequential blocks:

```
2 bytes: Block length (little-endian)
1 byte:  Flag (0x00 = header, 0xFF = data)
N bytes: Data
1 byte:  Checksum (XOR of flag and data)
```

### TZX Format

More complex, supports custom loaders, turbo loading, direct recording.

### SNA Format (Snapshot)

Fixed 49179 byte format:
- 27 bytes: Header (registers)
- 49152 bytes: RAM ($4000-$FFFF)

### Z80 Format (Snapshot)

Variable format with compression, supports 128K.

## Verification Files

```
# Firmware
Sinclair ZX Spectrum/Firmware/ZX Spectrum (1982)(Sinclair Research).rom
Sinclair ZX Spectrum/Firmware/ZX Spectrum +2 (1986)(Amstrad).rom

# Test software
Sinclair ZX Spectrum/Applications/ZEXALL (1994)(Woodmass, Frank).tap

# Games (timing sensitive)
Sinclair ZX Spectrum/Games/Manic Miner (1983)(Bug-Byte Software).tap
Sinclair ZX Spectrum/Games/Jet Set Willy (1984)(Software Projects).tap
Sinclair ZX Spectrum/Games/Elite (1985)(Firebird Software).tap
Sinclair ZX Spectrum/Games/Chase H.Q. (1989)(Ocean Software)[128K].tap

# Demos (highly timing sensitive)
Sinclair ZX Spectrum/Demos/Shock Megademo (1990)(Raww Arse).tap
```

## Common Pitfalls

1. **Screen memory layout** — The interleaved format catches everyone.
2. **Contention timing** — Easy to get wrong, breaks demos.
3. **Attribute flash** — Alternates every 16 frames.
4. **128K paging lock** — Once bit 5 is set, only reset unlocks.
5. **Interrupt timing** — INT fires at specific point in frame, duration matters.
