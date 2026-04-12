# Amiga Custom Chipset — Behavioral Reference

How the Amiga's custom chips behave cycle-by-cycle. This is not a register
reference (the HRM covers that) — it documents the timing, sequencing, and
inter-chip interactions that a cycle-accurate emulator must get right.

## Chip Inventory

### Custom Chips

| Chip | OCS Name | ECS Name | AGA Name | Role |
|------|----------|----------|----------|------|
| Agnus | 8361 (NTSC) / 8367 (PAL) | 8372A (Super Agnus) | 8374 (Alice) | Beam counter, DMA controller, copper, blitter |
| Denise | 8362 | 8373 (Super Denise) | 4203 (Lisa) | Video output, bitplanes, sprites, collisions |
| Paula | 8364 | 8364 | 8364 | Interrupts, audio DMA, disk DMA |
| CIA | 8520-A, 8520-B | 8520-A, 8520-B | 8520-A, 8520-B | Timers, I/O ports, TOD, serial |

Paula and the CIAs are unchanged across OCS/ECS/AGA. Agnus and Denise gain
features with each generation but remain backward-compatible — ECS wraps OCS,
AGA wraps ECS.

### Support Chips

| Chip | Found in | Role |
|------|----------|------|
| Gary | All models | Address decoder — routes bus cycles to the correct chip |
| Fat Gary | A3000, A4000 | Enhanced address decoder + bus timeout + resource registers |
| RAMSEY | A3000, A4000 | DRAM controller for fast RAM + config/revision registers |
| Gayle | A600, A1200 | IDE hard disk interface + PCMCIA slot |
| DMAC 390537 | A3000 | SCSI DMA controller (WD33C93 + DMA engine) |
| Buster | A500, A2000 | Zorro II expansion bus controller (autoconfig) |
| Super Buster | A3000, A4000 | Zorro II + III expansion bus controller |

## Clock Domains

Everything derives from a single crystal:

```
Crystal: 28.37516 MHz (PAL) / 28.63636 MHz (NTSC)
   ÷4 → Colour clock (CCK): 7.09 MHz — the fundamental DMA timing unit
   ÷8 → CPU clock: 3.55 MHz — one CPU cycle = 2 CCKs
```

One colour clock = one DMA slot. All chip-bus transactions, DMA fetches, and
register accesses happen on CCK boundaries. The CPU gets the bus when no DMA
channel needs it.

### Timing Relationships

| Unit | PAL | NTSC | Derivation |
|------|-----|------|------------|
| Crystal | 28.37516 MHz | 28.63636 MHz | Master oscillator |
| CCK | 7.09379 MHz | 7.15909 MHz | Crystal ÷ 4 |
| CPU clock | 3.546895 MHz | 3.579545 MHz | Crystal ÷ 8 |
| CCKs per line | 227 | 227 | Fixed |
| Lines per frame | 312 (313 long) | 262 (263 long) | PAL/NTSC standard |
| CCKs per frame | 70,884 (71,111) | 59,474 (59,701) | CCKs/line × lines |
| CPU cycles per line | 113.5 | 113.5 | 227 CCKs ÷ 2 |

The half-cycle at the end of each line means the CPU alternates between 113 and
114 cycles per line. This matters for programs that count exact cycle timings.

### Pixel Clocks

| Resolution | Pixels per CCK | Pixel frequency | Derivation |
|------------|---------------|-----------------|------------|
| Lores | 2 | 14.19 MHz | Crystal ÷ 2 |
| Hires | 4 | 28.38 MHz | Crystal ÷ 1 |
| SuperHires (ECS) | 8 | 56.75 MHz | Crystal × 2 |

Lores pixels are 4 superhires pixels wide. Hires pixels are 2 superhires
pixels wide. The raster framebuffer stores everything at superhires resolution
to avoid alignment errors.

## The Chip Bus

All four chips share a single 16-bit data bus. Agnus owns the address lines and
arbitrates who gets the bus each CCK:

```
           ┌─────────┐
Crystal ──▶│  Agnus   │──── Address bus (21 bits) ────┐
           │ (DMA     │                                │
           │  arbiter)│──── DMA slot grant ──▶ Paula   │
           └────┬─────┘                                │
                │                                      ▼
                │ ◀── Data bus (16 bits) ──▶     Chip RAM
                │                                      ▲
                ▼                                      │
           ┌─────────┐                                 │
           │ Denise   │ ◀── Bitplane data ─────────────┘
           │ (video)  │
           └─────────┘
```

**Key rule:** only one chip-bus transaction per CCK. If Agnus gives a slot to
disk DMA, the CPU waits. If the blitter is running, the CPU may be locked out
for the duration of the blit (blitter nasty mode) or may get odd-numbered slots.

## Document Index

### Custom Chips

| Document | Coverage |
|----------|----------|
| [agnus.md](agnus.md) | Beam counter, DMA slot allocation, copper, blitter |
| [denise.md](denise.md) | Video output, bitplanes, sprites, collisions, display modes |
| [paula.md](paula.md) | Interrupt priority, audio DMA pipeline, disk DMA |
| [cia.md](cia.md) | CIA 8520 timers, I/O ports, TOD, serial, keyboard protocol |

### Support Chips

| Document | Coverage |
|----------|----------|
| [gary.md](gary.md) | Address decoder, chip-select map, CIA decode, model configs |
| [fat-gary.md](fat-gary.md) | Bus timeout (TOENB), resource registers, 24-bit bus gate |
| [ramsey.md](ramsey.md) | DRAM controller, config/revision registers at $DE0000 |
| [gayle.md](gayle.md) | IDE interface, PCMCIA slot (SRAM, CompactFlash, NE2000) |
| [dmac.md](dmac.md) | SCSI DMA controller (WD33C93 + SDMAC registers) |
| [buster.md](buster.md) | Zorro II/III autoconfig, expansion bus, board dispatch |

### System-Level

| Document | Coverage |
|----------|----------|
| [inter-chip-timing.md](inter-chip-timing.md) | Multi-chip sequences: DMA→display, interrupts, copper, frame timing |
| [memory-map.md](memory-map.md) | Full address map for all models side by side |
| [floppy.md](floppy.md) | Floppy drive: MFM encoding, motor control, disk DMA integration |
| [keyboard.md](keyboard.md) | Keyboard controller: serial protocol, power-up, handshake |

## OCS → ECS → AGA

Each generation adds features while maintaining backward compatibility:

### ECS (A500+, A600, A3000)

- **Agnus (Super Agnus):** Programmable beam counter (BEAMCON0), 1 MB chip RAM
  addressing, ECS blitter size registers (BLTSIZV/BLTSIZH for >1024×1024 blits)
- **Denise (Super Denise):** DENISEID register ($DFF07C = $FC), SuperHires mode,
  BPLCON3 (border blank, kill EHB), programmable sync generation

### AGA (A1200, A4000, CD32)

- **Agnus (Alice):** 2 MB chip RAM, FMODE (wider DMA fetches: 32/64-bit),
  bitplanes 7-8 in free DMA slots
- **Denise (Lisa):** DENISEID = $F8, 256-entry 24-bit palette (via BPLCON3
  bank select + LOCT), HAM8, 8 bitplanes, BPLCON4 palette XOR, wider sprites
  (32/64 pixels via FMODE)
