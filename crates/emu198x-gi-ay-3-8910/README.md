# emu198x-gi-ay-3-8910

General Instrument AY-3-8910 PSG.

The Programmable Sound Generator used by the ZX Spectrum 128, Amstrad CPC,
MSX, and many arcade boards: three square-wave tone channels, a noise
generator, a shared envelope generator, and two 8-bit bidirectional I/O ports
(the 8912 variant exposes only port A).

```rust
use emu198x_gi_ay_3_8910::Ay;

let mut ay = Ay::new();
ay.select_register(7);
ay.write_data(0b0011_1000); // tone on, noise off
```

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
