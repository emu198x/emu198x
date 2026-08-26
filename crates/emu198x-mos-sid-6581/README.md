# emu198x-mos-sid-6581

MOS 6581 / 8580 SID.

The Commodore 64's Sound Interface Device: three voices with independent
waveform, ADSR envelope and ring/sync modulation, feeding a multi-mode
(low/band/high-pass) analogue filter. Both the 6581 and the later 8580 are
modelled, including their differing filter and combined-waveform behaviour.

Output is downsampled to a host sample rate, so the crate is usable as an
audio source without further resampling.

```rust
use emu198x_mos_sid_6581::Sid;

let mut sid = Sid::new();
sid.write(0x18, 0x0F);      // volume
let sample = sid.read(0x1B); // OSC3 / random
```

A `.sid` tune is 6502 machine code, so a player needs a CPU alongside this —
see [`emu198x-mos-6502`](https://crates.io/crates/emu198x-mos-6502).

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
