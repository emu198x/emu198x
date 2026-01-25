# MEGA65 Architecture Research

## Overview

The MEGA65 is a modern recreation of the unreleased Commodore 65, implemented in FPGA.
It extends the C64/C65 architecture with significant enhancements while maintaining
backwards compatibility.

## Hardware Summary

| Component | Specification |
|-----------|--------------|
| CPU | 45GS02 @ 48MHz (4502/65CE02 compatible) |
| RAM | 384KB main + 8MB Hyper RAM |
| Color RAM | 32KB |
| Video | VIC-IV (1920x1200 max, 256 colors from 24-bit palette) |
| Audio | Dual SID (optional) |
| Storage | SD card, 1541/1571 compatible |

## CPU: 45GS02 (Complexity: HIGH)

### Lineage
```
6502 → 65C02 → 65CE02/4502 → 4510 → 45GS02
```

### New Registers (vs 6502)
- **Z register** - Third 8-bit index register
- **B register** - Base page register (relocatable zero page)
- **16-bit Stack Pointer** (extended from 8-bit)

### New Instructions (~30+ new opcodes)

| Opcode | Mnemonic | Description |
|--------|----------|-------------|
| $C2 | CPZ | Compare Z register |
| $32 | DEZ | Decrement Z |
| $1A | INZ | Increment Z |
| $A3 | LDZ | Load Z |
| $64 | STZ | Store Z (or store zero) |
| $4B | TAZ | Transfer A to Z |
| $6B | TZA | Transfer Z to A |
| $DB | PHZ | Push Z |
| $FB | PLZ | Pull Z |
| $5B | TAB | Transfer A to Base page |
| $7B | TBA | Transfer Base page to A |
| $0B | TSY | Transfer SP to Y (16-bit) |
| $2B | TYS | Transfer Y to SP |
| $F4 | PHW | Push word (16-bit) |
| $FC | PHW | Push word (absolute) |
| $42 | NEG | Negate accumulator |
| $44 | ASR | Arithmetic shift right |
| $CB | ASW | Arithmetic shift left 16-bit |
| $EB | ROW | Rotate left 16-bit |
| $C3 | DEW | Decrement word |
| $E3 | INW | Increment word |
| $63 | BSR | Branch to subroutine |
| $14 | TRB | Test and reset bits |
| $1C | TSB | Test and set bits |

### 45GS02 Extensions (beyond 65CE02)

1. **32-bit Addressing**: NOP prefix ($EA) enables 32-bit ZP indirect
2. **Quad Register (Q)**: NEG NEG prefix ($42 $42) enables 32-bit operations
3. **28-bit Address Space**: 256MB addressable
4. **Hypervisor Mode**: Privileged instructions for OS/hypervisor
5. **Hardware Math**: 32-bit multiplier/divider registers

### Implementation Effort for CPU

**Minimum viable (65CE02 compatible):**
- ~30 new opcodes
- New Z register and operations
- Relocatable zero page (B register)
- 16-bit stack pointer
- New addressing modes (stack-relative, Z-indexed)
- Modified cycle counts (faster single-byte instructions)

**Full 45GS02:**
- 32-bit addressing prefix handling
- Q register (virtual 32-bit A+X+Y+Z)
- MMU integration
- Hypervisor traps
- Hardware math registers

**Estimate: 500-1000 lines for 65CE02, additional 300-500 for 45GS02 extensions**

## Video: VIC-IV (Complexity: VERY HIGH)

### Compatibility Layers
1. **VIC-II mode** - Full C64 compatibility
2. **VIC-III mode** - C65 modes (80-column, 640x200 bitmap)
3. **VIC-IV mode** - MEGA65 enhanced modes

### Key Features

| Feature | Description |
|---------|-------------|
| Resolution | Up to 1920x1200 |
| Colors | 256 from 24-bit palette |
| Character sets | 8,192 unique characters |
| Sprites | Enhanced VIC-II compatible |
| Bitplanes | VIC-III compatible |
| Full-color text | 256-color characters |

### Register Access
```
$D02F = $A5, $96  → Unlock VIC-III registers ($D030-$D07F)
$D02F = $47, $53  → Unlock VIC-IV registers
```

### 16-bit Character Mode
- 32 bits per character (2 bytes screen + 2 bytes color RAM)
- Per-character flip, alpha blend, repositioning
- "GOTO" mode for text overlays

### Implementation Effort

The VIC-IV is the most complex component. It needs to:
- Maintain full VIC-II compatibility (already have this)
- Add VIC-III bitplane modes
- Implement 16-bit character mode
- Support 256-color palette with 24-bit space
- Handle multiple resolution modes
- Support sprite enhancements

**Estimate: 2000-4000 lines, high complexity**

## Memory Map (Complexity: MEDIUM-HIGH)

### Address Space
- 28-bit (256MB) with banking
- Multiple memory "maps" switchable
- Hypervisor memory protection

### Banking
- Compatible with C64/C128 banking
- Extended MAP instruction for 28-bit remapping
- DMA controller for fast transfers

**Estimate: 400-600 lines extending current MMU**

## Comparison to Current Implementation

| Component | Current Status | MEGA65 Requirement |
|-----------|---------------|-------------------|
| CPU (6502) | Complete | Need 65CE02 + 45GS02 extensions |
| VIC-II | ~60% | Need VIC-III/IV modes |
| SID | ~95% | Compatible, add dual SID |
| CIA | ~30% | Compatible |
| Memory | 128K (C128) | 384KB + 8MB expansion |
| Disk | D64/D71 | Add D81 (3.5" 1581 format) |

## Implementation Roadmap (if pursued)

### Phase 1: 65CE02 CPU (Medium effort)
1. Add Z register
2. Implement new opcodes (~30)
3. Add stack-relative addressing
4. Support relocatable zero page

### Phase 2: Basic C65 Support (High effort)
1. Extend VIC to VIC-III modes
2. Add DMAgic (DMA controller)
3. Implement D81 disk format
4. 128KB RAM support

### Phase 3: Full MEGA65 (Very High effort)
1. 45GS02 extensions (32-bit, Q register)
2. VIC-IV full implementation
3. Hypervisor mode
4. 8MB expansion RAM
5. SD card interface

## Difficulty Assessment

| Task | Difficulty | Lines of Code (est.) |
|------|------------|---------------------|
| 65CE02 CPU | Medium | 500-800 |
| 45GS02 extensions | Medium-High | 300-500 |
| VIC-III modes | High | 800-1200 |
| VIC-IV modes | Very High | 1500-2500 |
| DMAgic | Medium | 300-500 |
| Memory/MMU | Medium | 400-600 |
| D81 disk | Low | 200-300 |
| **Total** | **Very High** | **4000-6500** |

## Existing Resources

### Xemu Emulator
- Open source MEGA65 emulator (GPL v2)
- Repository: https://github.com/lgblgblgb/xemu
- Written in C, uses SDL2
- Can reference for implementation details

### Documentation
- [MEGA65 User Guide](https://github.com/MEGA65/mega65-user-guide)
- [MEGA65 Core](https://github.com/MEGA65/mega65-core) - FPGA implementation
- [C65 Specifications](https://github.com/MEGA65/c65-specifications)

## Recommendation

**For C64/C128 emulation focus:** Not recommended due to high complexity.
The VIC-IV alone would require more code than the entire current VIC-II.

**For educational/hobby project:** Could implement 65CE02 CPU as an
interesting extension, which would enable basic C65 compatibility.

**Phased approach if pursued:**
1. Start with 65CE02 CPU (most transferable knowledge)
2. Add VIC-III 80-column mode (extends VDC work)
3. Evaluate VIC-IV effort after Phase 2

## Sources

- [MEGA65 Wiki (C64-Wiki)](https://www.c64-wiki.com/wiki/MEGA65)
- [MEGA65 Hardware Specifications](https://www.vintageisthenewold.com/mega65-further-details-and-hardware-specifications)
- [65CE02 Wikipedia](https://en.wikipedia.org/wiki/CSG_65CE02)
- [65CE02 Opcodes](https://www.oxyron.de/html/opcodesce02.html)
- [VIC-IV Modes Documentation](https://github.com/MEGA65/mega65-core/blob/master/docs/viciv-modes.md)
- [Xemu Emulator](https://github.com/lgblgblgb/xemu)
