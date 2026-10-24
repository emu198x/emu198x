# emu198x-motorola-6809

Motorola MC6809.

The 8-bit CPU behind the Dragon 32 and 64 and the Tandy Color Computer.
`tick_phase` advances one E/Q clock phase and `tick` one whole bus cycle, so
bus activity lands on the cycle the hardware would drive it.

```rust
use emu198x_motorola_6809::Mc6809;

let mut cpu = Mc6809::new();
```

Memory is supplied by the caller between ticks: the core exposes its bus state
as public fields and owns no bus, so the same CPU serves machines whose memory
maps differ completely.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
