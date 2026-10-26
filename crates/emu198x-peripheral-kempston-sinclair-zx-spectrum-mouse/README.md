# emu198x-peripheral-kempston-sinclair-zx-spectrum-mouse

Kempston Micro Electronics' Kempston mouse interface: two free-running 8-bit
quadrature counters and the buttons, on three partially decoded ports.

It implements the `Peripheral` trait from `emu198x-bus-sinclair-zx-spectrum`,
so any Spectrum-family machine that owns one calls its hooks from its own bus
handler. It depends on no machine.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
