# emu198x-sharp-lr35902

Sharp LR35902, the SM83 core.

The CPU inside the Game Boy's system-on-chip. One `tick` advances one machine
cycle, four T-cycles, which is the granularity the SoC's bus actually moves at.

```rust
use emu198x_sharp_lr35902::Sm83;

let mut cpu = Sm83::new();
```

Memory is supplied by the caller between ticks: the core exposes its bus state
as public fields and owns no bus, so the machine decides what each address
reaches.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
