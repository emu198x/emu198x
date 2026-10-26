# emu198x-sinclair-ula-7k010e

The Sinclair 7K010E, the ZX Spectrum 128K and +2's ULA. It has the 48K's
contention model on the 128K's longer line and frame, with its own
interrupt and contention phase.

It implements the `Ula` trait from `emu198x-bus-sinclair-zx-spectrum` over the
shared video engine in `emu198x-sinclair-zx-spectrum-ula`, and adds the
variant's own contention and port decoding. The machine ticks it every
half-cycle before the CPU and reads its pins between ticks: no bus callbacks,
the same as every Emu198x chip. It renders palette indices; RGBA conversion is
a separate stage.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
