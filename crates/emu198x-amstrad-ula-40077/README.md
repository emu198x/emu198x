# emu198x-amstrad-ula-40077

The Amstrad 40077 gate array, the ZX Spectrum +2A, +2B and +3's ULA. It
keeps the 128K's display geometry but has a contention pattern of its own,
and no contention on I/O.

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
