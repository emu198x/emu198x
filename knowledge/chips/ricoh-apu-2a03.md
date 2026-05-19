# Ricoh 2A03 APU

NES Audio Processing Unit — lives on the 2A03 CPU die. Two pulse channels, one triangle, one noise, and a DMC (delta modulation) channel. Ticked once per CPU cycle (~1.789 MHz NTSC). Output mixed through a non-linear mixer and downsampled to 48 kHz.

## Crate

`ricoh-apu-2a03` — **clean lift from archive.** No interface changes needed — the APU's API (tick per CPU cycle, register read/write with absolute addresses, buffered f32 output) is already the right shape for the pin-level machine layer.

### Test coverage

21 tests covering: silent default output, pulse/triangle/noise audio production, $4015 status register, channel enable/disable, frame counter IRQ (4-step mode), no IRQ in 5-step mode, buffer drain, DMC direct load, rate table, address/length formulas, DMC enable/disable/status/timer/loop/IRQ, and $4017 write delay.

## Channels

- **Pulse 1 & 2** — 8-step duty cycle sequencer (12.5%, 25%, 50%, 75%), envelope, length counter, sweep (pulse 1 uses one's complement negate, pulse 2 uses two's complement).
- **Triangle** — 32-step waveform (0-15-0), linear counter + length counter. Clocked every CPU cycle (not APU cycle).
- **Noise** — 15-bit LFSR with mode-selectable feedback tap (bit 1 or bit 6). Envelope, length counter.
- **DMC** — 1-bit delta-encoded samples from PRG memory via DMA. `dma_pending` flag signals the machine layer to feed a byte from `current_address`. Rate table indexed by $4010[3:0].

## Frame counter

Divides CPU cycles into quarter-frame (envelope, linear counter) and half-frame (length counter, sweep) events. Two modes: 4-step (generates IRQ at step 3) and 5-step (no IRQ). Mode change via $4017 is delayed 3-4 CPU cycles.

## Mixer

Non-linear mixer per nesdev formula — pulse channels through one lookup, triangle/noise/DMC through another. DC-blocking high-pass filter on the output.

## Related

- [MOS 6502](mos-6502.md) — the CPU (2A03 variant with BCD disabled, APU on-die)
- [Ricoh 2C02 PPU](ricoh-ppu-2c02.md) — the PPU
- [NES system overview](../systems/nintendo-nes.md)
