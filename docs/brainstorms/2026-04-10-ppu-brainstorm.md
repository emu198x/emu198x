# PPU Implementation Brainstorm

**Date:** 2026-04-10

## What We're Building

A pixel FIFO-based PPU for the Game Boy emulator. First implementation covers background tiles only — enough to display Tetris. Window layer and sprites come later.

The PPU ticks every T-cycle (matching the master oscillator), producing one pixel per dot during mode 3. Output is a 160x144 framebuffer of 2-bit palette indices, displayed via SDL3.

## Why This Approach

**Pixel FIFO over scanline renderer:** The rules require tick-by-tick accuracy. A scanline renderer would violate rule 16 ("no scanline-at-a-time shortcuts") and would fail on games that change scroll registers mid-scanline. The FIFO matches how the real PPU works — a fetcher fills the FIFO, and the LCD consumes one pixel per dot.

**BG only for first pass:** Tetris uses only background tiles. Adding window and sprites introduces FIFO stalling, priority logic, and OAM timing that we don't need yet. Layering these on later is straightforward — the FIFO architecture supports it by design.

**2-bit framebuffer:** Store raw palette indices, not RGBA. The PPU doesn't know about colours — it produces pixel values 0-3. Palette mapping happens at display time. This means palette register writes affect the next frame without re-rendering, matching hardware behaviour.

**SDL3 (system library):** Already installed (3.4.4). Link via build.zig. Handles window management, texture upload, and eventually audio and joypad input.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Rendering model | Pixel FIFO | Hardware accuracy, rules compliance |
| First scope | BG tiles only | Minimal for Tetris, layer on later |
| Framebuffer format | 2-bit indices `[144][160]u2` | Faithful to hardware, palette-independent |
| Display | SDL3, system-linked | Already installed, clean API, handles future audio/input |
| FIFO structure | 8-pixel shift register | Matches real hardware, fetcher pushes 8 pixels at a time |

## PPU Architecture

### Mode State Machine (per scanline of 456 dots)

```
Mode 2 (OAM scan):     dots 0-79 (80 dots fixed)
  - Search OAM for sprites on this scanline
  - First pass: skip (no sprites yet)

Mode 3 (pixel transfer): dots 80+ (variable, 172-289 dots)
  - BG fetcher runs: 4 states, 2 dots each
    1. Fetch tile ID from tile map
    2. Fetch tile data low byte
    3. Fetch tile data high byte
    4. Push 8 pixels to BG FIFO (if FIFO has room)
  - LCD consumes 1 pixel per dot from FIFO
  - SCX fine scroll discards first (SCX % 8) pixels
  - Mode 3 ends after 160 pixels pushed to LCD

Mode 0 (HBLANK):        remaining dots until 456
  - Idle

Mode 1 (VBLANK):         scanlines 144-153
  - 10 scanlines of idle
  - Trigger VBLANK interrupt
  - Upload framebuffer to SDL
```

### BG Fetcher States

```
State 0 (2 dots): Read tile ID
  - Address: 0x9800 + ((LY + SCY) / 8 * 32) + ((pixel_x + SCX) / 8)
  - Or 0x9C00 if LCDC bit 3 is set

State 1 (2 dots): Read tile data low byte
  - Address depends on LCDC bit 4 (tile data area)
  - Row within tile: (LY + SCY) % 8

State 2 (2 dots): Read tile data high byte
  - Address: tile_data_low_addr + 1

State 3 (2 dots): Push to FIFO
  - Combine low + high bytes into 8 two-bit pixels
  - Push only if FIFO has <= 8 pixels (room for 8 more)
  - If FIFO full, stall (repeat state 3)
```

### VRAM Layout

```
0x8000-0x87FF: Tile data block 0 (tiles 0-127)
0x8800-0x8FFF: Tile data block 1 (tiles 128-255, or signed -128 to 127)
0x9000-0x97FF: Tile data block 2 (tiles 0-127 when using signed addressing)
0x9800-0x9BFF: Tile map 0 (32x32 tile IDs)
0x9C00-0x9FFF: Tile map 1 (32x32 tile IDs)
```

### Registers to Implement

| Address | Register | Purpose |
|---------|----------|---------|
| FF40 | LCDC | LCD control (enable, tile data area, tile map area, BG enable) |
| FF41 | STAT | LCD status (mode, LYC match) — read only for now |
| FF42 | SCY | Scroll Y |
| FF43 | SCX | Scroll X |
| FF44 | LY | Current scanline (read only) |
| FF45 | LYC | LY compare (for STAT interrupt, later) |
| FF47 | BGP | BG palette (maps 0-3 to shades) |

## Open Questions

- **Tile data addressing:** LCDC bit 4 selects between unsigned (0x8000 base) and signed (0x8800 base + signed offset) tile addressing. Need to verify which mode Tetris uses.
- **SCX fine scroll timing:** The first tile fetch is offset by SCX % 8 pixels which are discarded. Does this add extra dots to mode 3? (Yes, it does — need to account for this.)
- **VRAM access during mode 3:** The CPU can't read VRAM during pixel transfer. Do we need to enforce this for Tetris? (Probably not initially, but it's a known accuracy concern.)

## Next Steps

1. Implement the pixel FIFO and BG fetcher in `src/ppu.zig`
2. Add LCDC, SCY, SCX, BGP register mapping in the bus
3. Wire up SDL3 in `build.zig` and create a display loop in `main.zig`
4. Run Tetris and see tiles
