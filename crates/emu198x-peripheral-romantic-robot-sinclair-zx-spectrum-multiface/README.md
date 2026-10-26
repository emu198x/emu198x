# emu198x-peripheral-romantic-robot-sinclair-zx-spectrum-multiface

Romantic Robot's Multiface 128 and Multiface 3: 8 KB of ROM and 8 KB of RAM
paged over the bottom 16 KB when the red button's NMI fetches `$0066`. The
ROM image comes from the user and is never part of this crate.

It implements the `Peripheral` trait and its `MemoryIntercept` from `emu198x-bus-sinclair-zx-spectrum`,
so any Spectrum-family machine that owns one calls its hooks from its own bus
handler. It depends on no machine.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
