# emu198x-mos-6502

MOS 6502.

The 8-bit CPU behind the Commodore 64, the NES, the BBC Micro, the Atari
800 and much else. Cycle-stepped rather than instruction-stepped, so bus
activity lands on the cycle the hardware would drive it.

```rust
use emu198x_mos_6502::M6502;

let mut cpu = M6502::new();
// `M6502::new_2a03()` selects the NES variant.
```

Memory is supplied by the caller each cycle; the core owns no bus, which is
what lets the same CPU serve machines whose memory maps differ completely.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
