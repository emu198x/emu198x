# Nintendo Entertainment System (NES)

## Status: Usable native/headless baseline

The NES path now has a usable NTSC machine/runtime/native baseline. It runs through the shared shell, passes the `nestest` CPU/machine proof, renders mapper-supported cartridges, and exposes screenshots, audio capture/playback, keyboard/gamepad input, reset, snapshots, local smoke-matrix reporting, Blargg-style `$6000` test ROM assertions, and video filter modes. Mapper coverage is still the main compatibility limiter.

## Hardware overview

- **CPU:** Ricoh 2A03 (6502 variant, no BCD mode, built-in audio)
- **Clock:** 21.477272 MHz master (NTSC), CPU at ÷12 (1.789773 MHz), PPU at ÷4 (5.369318 MHz)
- **Video:** PPU (2C02) — 256×240, 2 pattern tables (CHR ROM/RAM), 4 nametables, 64 sprites (8 per scanline), scrolling, palette of 64 colours (25 on screen)
- **Audio:** 2A03 built-in — 2 pulse channels, 1 triangle, 1 noise, 1 DPCM sample channel
- **Input:** Two controller ports (D-pad + A/B/Select/Start)
- **Storage:** Cartridge with mapper hardware (many variants)

## Work needed

- **6502 CPU / 2A03 variant** — done and validated by `nestest`
- **PPU** — dot-driven 2C02 path with nametable mirroring and frame output
- **APU** — 5-channel 2A03 audio with host-side channel controls
- **Mapper system** — NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM/BNROM, NINA-001, Sunsoft-4, and Camerica/Codemasters are implemented; the remaining long-tail mappers are compatibility-driven
- **iNES/NES 2.0** — ROM format parsing with mapper detection

## Automated test ROM checks

The headless `emu198x-nes` runner (`--no-default-features` skips the graphics stack) can assert Blargg-style test ROM output written at `$6000`. A passing ROM exits successfully and includes `test_result` in the JSON report; running, reset-requested, failed, or non-Blargg ROMs return a non-zero exit code.

```sh
cargo run --release -p emu198x-nes --no-default-features -- --rom apu_test.nes --frames 3000 --assert-blargg
cargo run --release -p emu198x-nes --no-default-features -- --smoke-root path/to/blargg/rom_singles --frames 1200 --assert-blargg --smoke-report tmp/nes-apu-blargg-report.json
```

## Crates

| Crate | Role | Status |
|-------|------|--------|
| `mos-6502` | Shared 6502 core with 2A03 mode | Done |
| `ricoh-ppu-2c02` | NES PPU | Ported |
| `ricoh-apu-2a03` | NES APU | Ported |
| `format-nintendo-nes-ines` | iNES parser + NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mappers | Active |
| `machine-nintendo-nes` | NES machine wiring | Active |
| `runtime-nintendo-nes` | Shared shell runtime | Active |
| `emu198x-nes` | Native verifier shell | Active |
