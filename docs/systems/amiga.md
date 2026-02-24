# Amiga

## Overview

| Property | Value |
|----------|-------|
| CPU | Motorola 68000 @ 7.09 MHz (PAL) |
| Crystal | 28.37516 MHz (PAL) / 28.63636 MHz (NTSC) |
| Chip RAM | 512K-2M (depending on model) |
| Video | OCS/ECS/AGA custom chips |
| Audio | Paula, 4 channels, 8-bit |
| Release | 1985 |

## Timing

### Crystal Derivation (PAL)

```
Crystal: 28.37516 MHz
   ÷4 → 7.09379 MHz (CPU clock)
   ÷8 → 3.546895 MHz (colour clock)
```

### Crystal Derivation (NTSC)

```
Crystal: 28.63636 MHz
   ÷4 → 7.15909 MHz (CPU clock)
   ÷8 → 3.579545 MHz (colour clock)
```

### Colour Clock

The **colour clock** (CCK) is the fundamental timing unit for the custom chips. All DMA slots, copper timing, and display events align to colour clocks.

| Region | Colour clock | Cycles per line | Lines per frame |
|--------|--------------|-----------------|-----------------|
| PAL | 3.55 MHz | 227.5 | 312.5 (interlaced) |
| NTSC | 3.58 MHz | 227.5 | 262.5 (interlaced) |

### CPU Bus Access

The 68000 runs at 4 colour clocks per CPU cycle minimum. Bus cycles take 4 CCKs (chip RAM) or 2 CCKs (fast RAM).

DMA has priority over CPU. When DMA is active, CPU waits.

### Phase Relationship

```
Crystal:      |0|1|2|3|4|5|6|7|
Colour clock: |*|.|.|.|.|.|.|.|  (every 8)
CPU clock:    |*|.|.|.|*|.|.|.|  (every 4, when bus free)
```

## Memory Map

### Address Space (24-bit, 68000)

| Address | Contents |
|---------|----------|
| $000000-$07FFFF | Chip RAM (512K) |
| $080000-$0FFFFF | Extended chip RAM (A500+) |
| $100000-$1FFFFF | Extended chip RAM (A1200) |
| $BFD000 | CIA-A (odd addresses) |
| $BFE001 | CIA-B (even addresses) |
| $C00000-$DFFFFF | Slow RAM (A500 trapdoor) |
| $DFF000-$DFF1FF | Custom chip registers |
| $E00000-$E7FFFF | Reserved |
| $E80000-$EFFFFF | Autoconfig space |
| $F00000-$F7FFFF | Reserved |
| $F80000-$FFFFFF | Kickstart ROM (256K/512K) |

### Kickstart Mapping

- A1000: $F80000-$FFFFFF (loaded from disk)
- A500/A2000: $F80000-$FFFFFF (256K) or $F80000-$FFFFFF (512K)
- A1200/A4000: $F80000-$FFFFFF (512K)

## Custom Chips

### Agnus (DMA Controller)

Controls all DMA transfers:
- Bitplane DMA
- Sprite DMA
- Blitter
- Copper
- Disk
- Audio

### Denise (Video)

Generates video output:
- Bitplane to pixel conversion
- Sprite rendering
- Collision detection
- Colour palette

### Paula (Audio + Disk + Interrupts)

- 4 DMA audio channels
- Disk controller
- Interrupt controller

## Custom Registers ($DFF000+)

### Frequently Used

| Offset | Name | Access | Function |
|--------|------|--------|----------|
| $000 | BLTDDAT | R | Blitter destination data |
| $004 | VPOSR | R | Vertical beam position (high) |
| $006 | VHPOSR | R | Vertical/horizontal beam |
| $02A | VPOSW | W | Vertical beam position write |
| $02C | COPCON | W | Copper control |
| $080 | COP1LCH | W | Copper list 1 address high |
| $082 | COP1LCL | W | Copper list 1 address low |
| $088 | COPJMP1 | W | Restart copper at COP1LC |
| $096 | DMACON | W | DMA control |
| $09A | INTENA | W | Interrupt enable |
| $09C | INTREQ | W | Interrupt request |
| $100 | BPLCON0 | W | Bitplane control |
| $102 | BPLCON1 | W | Bitplane scroll |
| $104 | BPLCON2 | W | Bitplane priority |
| $108 | BPL1MOD | W | Bitplane modulo odd |
| $10A | BPL2MOD | W | Bitplane modulo even |
| $0E0-$0EE | BPL1PT-BPL6PT | W | Bitplane pointers |
| $180-$1BE | COLOR00-31 | W | Colour palette |

### DMACON ($096)

```
Bit 15: SET/CLR (1 = set bits, 0 = clear bits)
Bit 9: DMAEN (master enable)
Bit 8: BPLEN (bitplane DMA)
Bit 7: COPEN (copper DMA)
Bit 6: BLTEN (blitter DMA)
Bit 5: SPREN (sprite DMA)
Bit 4: DSKEN (disk DMA)
Bit 3-0: AUDxEN (audio channels)
```

## Copper

The copper is a programmable coprocessor that modifies chip registers in sync with the video beam.

### Instructions

**MOVE:** Write value to register
```
First word: Register offset (bits 8-1)
Second word: Data
```

**WAIT:** Wait for beam position
```
First word: VP (7-0), HP (7-1), bit 0 = 0
Second word: VM (7-0), HM (7-1), BFD (bit 15), bit 0 = 0
```

**SKIP:** Skip next instruction if beam past position
```
First word: VP (7-0), HP (7-1), bit 0 = 0
Second word: VM (7-0), HM (7-1), BFD (bit 15), bit 0 = 1
```

### Timing

Copper executes one instruction per 4 colour clocks (bus slot dependent). Cannot write to registers < $080 (protected area) unless COPCON bit 1 set.

## Blitter

Hardware 2D graphics processor. Can copy, fill, and combine rectangular regions with arbitrary logic operations.

### Channels

- A: Source
- B: Source
- C: Source (usually destination for read-modify-write)
- D: Destination

### Operations

Uses 8 minterm bits to combine A, B, C sources with any boolean operation.

### Line Drawing

Blitter can draw lines using Bresenham's algorithm in hardware.

## Sprites

8 hardware sprites, 16 pixels wide, arbitrary height.

### Types

- 3-colour + transparent (single sprite)
- 15-colour (attached pair)

### Registers

Each sprite has:
- SPRxPT: Pointer to sprite data
- SPRxPOS: Vertical start, horizontal start
- SPRxCTL: Vertical stop, control
- SPRxDATA/DATB: Pixel data

## Audio (Paula)

### Channels

4 DMA channels, 2 left (0, 3), 2 right (1, 2).

### Per-Channel Registers

| Register | Function |
|----------|----------|
| AUDxLCH/L | Sample pointer |
| AUDxLEN | Sample length (words) |
| AUDxPER | Period (pitch) |
| AUDxVOL | Volume (0-64) |
| AUDxDAT | Direct data write |

### Modulation

Channels can modulate each other:
- Volume modulation
- Period modulation

## CIA Chips

Two MOS 8520 chips for I/O.

### CIA-A ($BFE001, odd addresses)

| Register | Function |
|----------|----------|
| PRA | Gameport, disk, LED |
| PRB | Parallel port |
| DDRA/B | Data direction |
| Timer A/B | Timers |
| TOD | Time of day |
| ICR | Interrupt control |

### CIA-B ($BFD000, even addresses)

| Register | Function |
|----------|----------|
| PRA | Disk select, motor, direction |
| PRB | Keyboard data |
| Timer A/B | Timers |
| ICR | Interrupt control (triggers NMI) |

## Disk

### MFM Encoding

Data stored using Modified Frequency Modulation:
- Sync word: $4489
- Sector: Header + data + checksums

### Track Format

- 11 sectors per track (880K per disk)
- Each sector: 512 bytes data

### DMA

Disk DMA transfers raw MFM data. Software decodes.

## Media Formats

### ADF (Amiga Disk File)

Raw track data, 880K:
```
80 tracks × 2 sides × 11 sectors × 512 bytes = 901,120 bytes
```

### ADZ

Gzip-compressed ADF.

### IPF (Interchangeable Preservation Format)

Preservation format capturing exact disk timing and copy protection.

### WHDLoad

Hard disk install format for games. Requires Kickstart and WHDLoad package.

## Kickstart Versions

| Version | Systems |
|---------|---------|
| 1.2 | A500, A2000 early |
| 1.3 | A500, A2000 |
| 2.04 | A500+ |
| 2.05 | A600 |
| 3.0 | A1200, A4000 |
| 3.1 | A1200, A4000 |

Games often require specific minimum Kickstart version.

## Verification Files

```
# Firmware
Commodore Amiga/Firmware/Kickstart v1.3 (1987)(Commodore)(A500-A1000-A2000).rom
Commodore Amiga/Firmware/Kickstart v3.1 (1993)(Commodore)(A1200).rom

# System software
Commodore Amiga/Applications/Workbench v1.3 (1988)(Commodore).adf
Commodore Amiga/Applications/Workbench v3.1 (1993)(Commodore).adf

# Games
Commodore Amiga/Games/Shadow of the Beast (1989)(Psygnosis).adf
Commodore Amiga/Games/Turrican (1990)(Rainbow Arts).adf
Commodore Amiga/Games/Lemmings (1991)(Psygnosis).adf

# Demos (timing sensitive)
Commodore Amiga/Demos/State of the Art (1992)(Spaceballs).adf
Commodore Amiga/Demos/Desert Dream (1993)(Kefrens).adf
```

## Common Pitfalls

1. **Copper timing** — Must align to colour clock boundaries.
2. **DMA contention** — CPU starves when DMA heavy.
3. **Blitter busy** — Must wait for blitter before accessing its registers.
4. **Kickstart dependencies** — Games may need specific version.
5. **CIA timer accuracy** — Games use for timing and protection.
6. **Sprite DMA slots** — Limited positions per line.
7. **Interlace** — PAL is actually 312.5 lines, alternating fields.

## Emu198x Status (A500/OCS Baseline)

The current implementation status and known accuracy gaps for the Emu198x Amiga
baseline are tracked in:

- `docs/systems/amiga-accuracy-status.md`

Useful local commands:

```sh
# KS1.3 screenshot regression (requires Kickstart ROM)
AMIGA_KS13_ROM=roms/kick13.rom \
  cargo test -p machine-amiga --test boot_kickstart test_boot_kick13 -- --ignored

# Headless insert-screen timing benchmark
cargo run --release -p amiga-runner -- \
  --rom roms/kick13.rom \
  --headless \
  --frames 300 \
  --bench-insert-screen \
  --mute
```
