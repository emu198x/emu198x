# emu198x-commodore-paula-8364

Commodore 8364 Paula.

One of the three custom chips in the Amiga's Original Chipset, owning three
register groups that share a die:

- **Interrupt controller** — INTENA/INTREQ, 14 sources mapped to 6 IPL levels
- **Audio** — four DMA-driven channels with ADKCON modulation
- **Floppy** — disk DMA and the MFM front-end

```rust
use emu198x_commodore_paula_8364::Paula;

let mut paula = Paula::new();
```

The three groups are separable: a consumer that only wants audio need not
drive the disk side.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
