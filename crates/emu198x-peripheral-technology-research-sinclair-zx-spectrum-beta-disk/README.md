# emu198x-peripheral-technology-research-sinclair-zx-spectrum-beta-disk

Technology Research's Beta 128 disk interface, the standard floppy system of
the Pentagon, Scorpion and other Eastern European Spectrum clones. It covers
TR-DOS ROM paging on M1 fetches in `$3D00-$3DFF`, the WD1793 controller with
`.trd` images, and the system register at port `$FF`.

It implements the `Peripheral` trait from `emu198x-bus-sinclair-zx-spectrum`,
so any Spectrum-family machine that owns one calls its hooks from its own bus
handler. It depends on no machine.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
