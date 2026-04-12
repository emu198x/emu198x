# Commodore Amiga

## Status: Not started

The most ambitious target in the project. The Amiga's custom chipset (Agnus, Denise, Paula) is DMA-driven with a copper co-processor, blitter, and bitplane graphics — fundamentally different from the register-based video of 8-bit systems.

## Hardware overview

### Variants

| Model | CPU | Chipset | Chip RAM | Year |
|-------|-----|---------|----------|------|
| A500 | 68000 @ 7.09 MHz | OCS (Agnus 8361, Denise 8362, Paula 8364) | 512KB–1MB | 1987 |
| A500+ | 68000 | ECS (Fat Agnus 8375) | 1MB | 1991 |
| A600 | 68000 | ECS | 1MB–2MB | 1992 |
| A2000 | 68000 | OCS/ECS | 1MB | 1987 |
| A1200 | 68EC020 @ 14 MHz | AGA (Alice, Lisa, Paula) | 2MB | 1992 |
| A3000 | 68030 @ 25 MHz | ECS | 2MB | 1990 |
| A4000 | 68040 @ 25 MHz | AGA | 2MB | 1992 |

### Core hardware (OCS A500)
- **CPU:** Motorola 68000 @ 7.09 MHz (PAL)
- **Clock:** 28.37516 MHz master (PAL). 68000 at ÷4, chipset at ÷2 (colour clock) and ÷4 (low-res pixel)
- **Video (Denise):** Bitplane graphics (1-6 planes), HAM (Hold-And-Modify), dual playfield, hardware sprites (8), copper co-processor for per-scanline register changes
- **Audio (Paula):** 4 channels, 8-bit PCM, DMA-driven from chip RAM, per-channel volume, period, and modulation
- **DMA (Agnus):** Central DMA controller arbitrating access between CPU, copper, blitter, sprites, audio, disk, and bitplane fetch. Cycle-exact scheduling.
- **Blitter:** Hardware block copy/fill with logic operations and line drawing. Channels A, B, C (source) → D (dest) with minterms.
- **Copper:** Co-processor executing a simple instruction set (MOVE, WAIT, SKIP) synchronised to the video beam position. Powers most Amiga visual effects.
- **Memory:** Chip RAM (shared between CPU and DMA) + Fast RAM (CPU only). OCS: 512KB chip max. ECS: 1MB/2MB.
- **Storage:** Custom MFM 3.5" floppy (880KB), ADF disk image format
- **System:** Kickstart ROM (256KB–512KB), boots to Workbench from floppy/HD

## Work needed

### CPU: Motorola 68000 — **Done** (`cpu-m68k`)
- 32-bit registers, 16-bit data bus, 24-bit address bus
- 8 data registers (D0-D7), 7 address registers (A0-A6), USP, SSP
- Supervisor/user modes, 7 interrupt priority levels with autovectoring
- All 8 effective address modes including indexed and PC-relative
- Full instruction set: MOVE, arithmetic, logic, shifts, bit ops, branches, MOVEM, MUL/DIV, LINK/UNLK, TRAP, EXG
- Later variants (68020/030/040) needed for A1200/A3000/A4000
- Test suites available: Musashi vectors — not yet run
- 14 tests passing

### Custom chipset: Agnus (DMA controller)
- DMA scheduling across all subsystems with cycle-exact priority
- Bitplane DMA fetch (interleaved with other DMA channels)
- Copper DMA (fetch and execute copper instructions)
- Blitter DMA (block operations with 4 channels)
- Sprite DMA, audio DMA, disk DMA
- Beam counter (VHPOSR/VHPOSW) drives all timing
- **Effort:** Very large — the hardest single component

### Custom chipset: Denise (video)
- Bitplane-to-pixel conversion (1-6 planes, dual playfield, HAM)
- 32-colour palette (OCS), 4096 colours via HAM
- 8 hardware sprites (attached sprites for 15-colour, sprite-playfield priority)
- Collision detection (sprite-sprite, sprite-playfield)
- Genlock support (optional)
- **Effort:** Large

### Custom chipset: Paula (audio + I/O)
- 4 DMA-driven 8-bit PCM channels
- Per-channel: period (frequency), volume, sample pointer, length
- Audio interrupts when buffer exhausted (for double-buffering)
- Disk I/O (MFM read/write)
- Serial port
- Interrupt controller
- **Effort:** Medium-large

### Blitter
- 4-channel DMA block operation engine (A, B, C sources → D dest)
- 256 logic operations via minterms
- Line drawing mode
- Area fill mode (inclusive/exclusive)
- Ascending/descending operation for overlapping regions
- **Effort:** Medium

### Copper
- 3-instruction co-processor: MOVE (write register), WAIT (wait for beam), SKIP (conditional skip)
- Beam position matching with configurable mask
- Danger bit controls access to upper/lower register set
- **Effort:** Small-medium

### Storage and system
- **ADF format** — raw MFM track images (880KB, 11 sectors × 80 tracks × 2 sides)
- **Kickstart ROM** — system firmware (multiple versions: 1.2, 1.3, 2.04, 2.05, 3.0, 3.1)
- **CIA 8520** — two CIAs for keyboard, parallel port, TOD clock, disk motor
- **Floppy controller** — custom MFM, DMA-driven via Paula
- **Effort:** Medium-large

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `cpu-m68k` | Motorola 68000 CPU core (68020/030/040 later) | Done |
| `commodore-agnus-ocs` | OCS Agnus DMA controller | Not started |
| `commodore-denise-ocs` | OCS Denise video | Not started |
| `commodore-paula` | Paula audio + I/O | Not started |
| `machine-commodore-amiga` | Amiga machine wiring | Not started |
| `format-commodore-amiga-adf` | ADF disk format | Not started |
| `emu198x-commodore-amiga` | GUI shell | Not started |

## ROMs needed

| File | Size | Description |
|------|------|-------------|
| `kick13.rom` | 256KB | Kickstart 1.3 (A500) |
| `kick20.rom` | 512KB | Kickstart 2.04/2.05 (A500+/A600) |
| `kick31.rom` | 512KB | Kickstart 3.1 (A1200/A4000) |
