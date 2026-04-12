# Nintendo Entertainment System (NES)

## Status: Not started

Depends on the 6502 CPU core (built for C64). The NES has simpler hardware than the C64 but the PPU (Picture Processing Unit) has complex scroll and sprite timing that many games exploit.

## Hardware overview

- **CPU:** Ricoh 2A03 (6502 variant, no BCD mode, built-in audio)
- **Clock:** 21.477272 MHz master (NTSC), CPU at ÷12 (1.789773 MHz), PPU at ÷4 (5.369318 MHz)
- **Video:** PPU (2C02) — 256×240, 2 pattern tables (CHR ROM/RAM), 4 nametables, 64 sprites (8 per scanline), scrolling, palette of 64 colours (25 on screen)
- **Audio:** 2A03 built-in — 2 pulse channels, 1 triangle, 1 noise, 1 DPCM sample channel
- **Input:** Two controller ports (D-pad + A/B/Select/Start)
- **Storage:** Cartridge with mapper hardware (many variants)

## Work needed

- **6502 CPU** — **Done** (shared with C64, `cpu-6502`; BCD flag needs disabling for 2A03 variant)
- **PPU** — tile-based renderer, sprite evaluation, scroll registers, nametable mirroring
- **APU** — 5-channel audio with frame counter
- **Mapper system** — cartridge mappers (NROM, MMC1, MMC3, etc. — dozens exist)
- **iNES/NES 2.0** — ROM format parsing with mapper detection

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `cpu-6502` | Shared with C64 | Done |
| `nintendo-ppu` | NES PPU |
| `machine-nintendo-nes` | NES machine wiring |
| `emu198x-nintendo-nes` | GUI shell |
