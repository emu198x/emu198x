# emu198x-ricoh-apu-2a03

Ricoh 2A03 APU.

The audio half of the NES's CPU die: two pulse channels, a triangle channel,
a noise channel, and a DMC channel that fetches delta-encoded samples over
DMA, stealing CPU cycles one byte at a time.

```rust
use emu198x_ricoh_apu_2a03::Apu;

let mut apu = Apu::new();
apu.write(0x4015, 0x0F); // enable the four tone channels
```

Ticked once per CPU cycle (~1.789 MHz NTSC).

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
