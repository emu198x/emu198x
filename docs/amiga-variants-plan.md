# Amiga Variant Plan

## Design principle

Model an Amiga as a configuration of independent axes, not as six separate emulators. Most hardware behaviour is identical across OCS/ECS/AGA — the differences are narrow and well-defined. Each component stores its variant as an enum field and dispatches at the few points where behaviour diverges.

No trait objects. No generic type parameters on the system struct. Just `match self.variant` where it matters.

## Variant axes

### Agnus (DMA controller / beam counter)

| Variant | Chip | Chip RAM DMA | Fetch width | Models |
|---------|------|-------------|-------------|--------|
| Agnus8361 | 8361 (NTSC) / 8367 (PAL) | 512K (18-bit) | 16-bit | A1000, A500, A2000 early |
| FatAgnus8371 | 8370/8371 | 1MB (19-bit) | 16-bit | A2000 late |
| Agnus8372 | 8372A | 2MB (20-bit) | 16-bit | A500+, A600 |
| Alice | Alice | 2MB (21-bit) | 64-bit | A1200, A4000 |

**What changes:**
- `chip_ram_dma_mask`: 18/19/20/21-bit address mask for DMA pointer registers
- `read_vposr()`: Agnus ID bits 8-14 (already implemented)
- Alice: 64-bit fetch mode (4 words per CCK instead of 1), needed for AGA 8-bitplane modes

**What doesn't change:** beam counter, DMA slot allocation, DMACON, copper/blitter DMA scheduling. All identical.

### Denise (video output)

| Variant | Chip | Colours | Bitplanes | Special modes |
|---------|------|---------|-----------|---------------|
| Denise8362 | 8362 | 4096 (12-bit) | 6 | HAM6, dual-playfield |
| SuperDenise8373 | 8373 | 4096 (12-bit) | 6 | + EHB, productivity modes, genlock |
| Lisa | Lisa | 16M (24-bit) | 8 | + HAM8, 256-colour, border blank |

**What changes:**
- Palette depth: 12-bit (OCS/ECS) vs 24-bit (AGA, via BPLCON3 bank select)
- Palette size: 32 (OCS/ECS) vs 256 (AGA)
- Max bitplanes: 6 (OCS/ECS) vs 8 (AGA)
- EHB mode (SuperDenise/Lisa): extra-half-brite, 64 colours from 32-entry palette
- HAM8 (Lisa): 256K colours via modify-in-place
- Sprite width: 16px (OCS/ECS) vs 16/32/64px (AGA)
- Scroll granularity: lo-res pixel (OCS/ECS) vs sub-pixel (AGA FMODE)

**What doesn't change:** basic bitplane-to-pixel shift register logic, sprite priority, dual-playfield. The machinery is the same, just wider/deeper.

### Paula (interrupts + audio + serial)

Identical across all models. No variant dispatch needed.

### CIA (8520)

Identical across all models. No variant dispatch needed.

### CPU

| Variant | CPU | Data bus | Caches | FPU | Models |
|---------|-----|----------|--------|-----|--------|
| M68000 | 68000 | 16-bit | none | none | A1000, A500, A500+, A600, A2000 |
| M68020 | 68020 | 32-bit | 256B I-cache | optional | A1200 |
| M68030 | 68030 | 32-bit | 256B I+D cache | optional | accelerator cards |
| M68040 | 68040 | 32-bit | 4KB I+D cache | integrated | A4000 |

**Current scope:** M68000 only. The 020/030/040 are future work (when emu-m68k adds support).

### Memory

| Model | Chip RAM | Slow RAM | Fast RAM | ROM size |
|-------|----------|----------|----------|----------|
| A1000 | 256K | 0 | 0 | 256K (WCS) |
| A500 | 512K | 0-512K (trapdoor) | 0 | 256K |
| A500+ | 1MB | 0 | 0 | 256K |
| A600 | 1MB (2MB opt) | 0 | 0-4MB (PCMCIA) | 512K |
| A2000 | 1MB | 0 | 0-8MB (Zorro) | 256K-512K |
| A1200 | 2MB | 0 | 0-8MB (trapdoor) | 512K |

**What changes:**
- `chip_ram_size` / `chip_ram_mask`: power-of-two size, address wrapping
- Slow RAM presence and mapping ($C00000-$D7FFFF)
- Fast RAM presence and mapping ($200000+ for A600 PCMCIA, Zorro autoconfig for A2000)
- ROM/WCS size: 256K or 512K
- Overlay mechanism: A1000 has external pull-up on OVL; A500+ has CIA-A PRA bit 0

**What doesn't change:** the basic memory.rs read/write dispatch. Chip RAM is always at $000000, ROM always at $F80000. Just the sizes and masks change.

### Kickstart compatibility

| KS Version | Supported models | Notes |
|------------|-----------------|-------|
| 1.0-1.1 | A1000 only | WCS, loaded from disk |
| 1.2 | A500, A2000 | First ROM Kickstart |
| 1.3 | A500, A2000 | Most common OCS KS |
| 2.04 | A500+ | ECS support |
| 2.05 | A600 | ECS, IDE support |
| 3.0 | A1200, A4000 | AGA support |
| 3.1 | A1200, A4000 | Final Commodore release |

KS 1.x checks Agnus ID at $DFF004 (VPOSR) to detect chipset. KS 2.x+ expects ECS registers. KS 3.x requires AGA. The emulator must return the correct Agnus ID for the configured variant.

## What's implemented

### config.rs (done)

Presets for A1000, A500, A500+, A600, A2000, A1200. Every field overridable. `KickstartSource::Wcs` for A1000, `KickstartSource::Rom` for everything else.

### Agnus (partial)

- Stores `AgnusVariant` enum
- `read_vposr()` returns correct Agnus ID per variant
- Beam counter: correct for PAL/NTSC (312/262 lines)
- DMA slot allocation: working for OCS
- **TODO:** `chip_ram_dma_mask` per variant (currently uses memory's chip_ram_mask)
- **TODO:** Alice 64-bit fetch mode

### Denise (OCS only)

- Stores `DeniseVariant` enum (unused — `#[allow(dead_code)]`)
- 32-entry 12-bit palette
- 6-bitplane shift register output
- **TODO:** EHB mode (SuperDenise)
- **TODO:** 256-entry 24-bit palette (AGA)
- **TODO:** 8 bitplanes (AGA)
- **TODO:** HAM6/HAM8
- **TODO:** Variable sprite width (AGA)

### Memory (done for OCS/ECS)

- Variable chip RAM size with power-of-two masking
- Slow RAM with wrapping
- WCS (writable) and ROM (read-only) Kickstart
- Overlay mechanism
- **TODO:** Fast RAM mapping (A600 PCMCIA, Zorro autoconfig)
- **TODO:** 512K ROM support (KS 2.x+)

### Bus (OCS only)

- CIA-A overlay control via PRA bit 0
- Custom register routing
- Chip RAM contention via wait_cycles
- **TODO:** Re-evaluate overlay after bus reset
- **TODO:** Fast RAM region (no contention)
- **TODO:** Autoconfig ($E80000)

## Phased rollout

### Phase A: OCS A500 with KS 1.3 (current focus)

Target: KS 1.3 boots to "insert disk" screen.

Hardware needed:
- Agnus8361, Denise8362, 68000, 512K chip + 512K slow, 256K ROM
- Copper (implemented), blitter (stub), CIA (implemented), Paula interrupts (implemented)
- Display: 1-6 bitplanes, dual-playfield, basic sprites
- Keyboard handshake (implemented), floppy (not needed for boot screen)

Verification: `--headless --frames 200 --screenshot` shows KS 1.3 gradient with "insert disk" text.

### Phase B: OCS A1000 with KS 1.0

Target: KS 1.0 boots from WCS.

Additional hardware:
- WCS RAM (writable Kickstart loaded from $1111 magic image)
- A1000 overlay (external pull-up, same as A500 but active at power-on)

Verification: KS 1.0 executes past RAM test, writes custom registers.

### Phase C: Floppy and Workbench

Target: Workbench 1.3 boots to desktop.

Additional hardware:
- Floppy DMA (disk.rs) — MFM decode, ADF loading
- Blitter — area copy, fill, line draw (needed for Workbench rendering)
- Full sprite DMA (mouse pointer)
- Audio DMA (basic, for system beep)

Verification: Workbench desktop renders with icons and mouse pointer.

### Phase D: ECS (A500+, A600)

Target: KS 2.04/2.05 boots on ECS configuration.

Additional hardware:
- Agnus8372: 2MB DMA address range
- SuperDenise8373: EHB mode, productivity display modes
- 1MB chip RAM default

What changes in code:
- `Agnus::chip_ram_dma_mask()` method returns mask per variant
- `Denise::output_pixel()` checks for EHB mode (bit 7 of BPLCON2) and halves palette index colours
- Bus returns ECS Agnus ID in VPOSR

Verification: KS 2.04 boots on A500+ preset. EHB demo renders correctly.

### Phase E: AGA (A1200)

Target: KS 3.1 boots on AGA configuration.

Additional hardware:
- Alice: 64-bit fetch (4 words per CCK in fetch window)
- Lisa: 256-entry 24-bit palette, 8 bitplanes, HAM8, variable sprite width
- 68020 CPU (requires emu-m68k 020 support)
- 2MB chip RAM, 512K ROM
- Autoconfig for Fast RAM

What changes in code:
- `Agnus::do_bitplane_fetch()` fetches 4 words per CCK in 64-bit mode
- `Denise` palette: `[u32; 256]` with bank select via BPLCON3
- `Denise::output_pixel()`: 8-bitplane index, HAM8 decode
- Bus: 32-bit data transfers (68020 long-word aligned)

Verification: KS 3.1 boots on A1200 preset. 256-colour Workbench screen.

## Dispatch examples

### Agnus chip RAM DMA mask (not yet implemented)

```rust
impl Agnus {
    pub fn chip_ram_dma_mask(&self) -> u32 {
        match self.variant {
            AgnusVariant::Agnus8361 => 0x0007_FFFE,   // 512K
            AgnusVariant::FatAgnus8371 => 0x000F_FFFE, // 1MB
            AgnusVariant::Agnus8372 => 0x001F_FFFE,    // 2MB
            AgnusVariant::Alice => 0x001F_FFFE,         // 2MB (same range, wider fetch)
        }
    }
}
```

### Denise EHB detection (not yet implemented)

```rust
impl Denise {
    fn is_ehb(&self) -> bool {
        matches!(self.variant, DeniseVariant::SuperDenise8373 | DeniseVariant::Lisa)
            && self.num_bitplanes() == 6
            && (self.bplcon2 & 0x0200) == 0  // not HAM
    }
}
```

### Read VPOSR with Agnus ID (implemented)

```rust
let agnus_id: u16 = match self.variant {
    AgnusVariant::Agnus8361 | AgnusVariant::FatAgnus8371 => 0x00,
    AgnusVariant::Agnus8372 => 0x20,
    AgnusVariant::Alice => 0x22,
};
```

## Rules

1. **OCS first, always.** Don't add ECS/AGA code until OCS boots Workbench.
2. **Variant dispatch at the leaves.** Don't branch on variant in the tick loop. Branch inside the component method that differs.
3. **Same struct, different enum.** Agnus is one struct for all variants. Denise is one struct. No AgnusOcs/AgnusEcs split.
4. **Presets are convenience, not enforcement.** `AmigaConfig::Custom` lets you mix any Agnus with any Denise. Real hardware didn't allow this, but accelerator cards did weird things.
5. **Test with real KS ROMs.** Each phase's verification is a real Kickstart booting to a known screen. No synthetic tests for chipset integration.
