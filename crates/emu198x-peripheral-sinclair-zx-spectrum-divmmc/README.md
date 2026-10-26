# emu198x-peripheral-sinclair-zx-spectrum-divmmc

The DivMMC SD card interface: an 8 KB EEPROM, 128 KB of banked RAM and an
SD card slot, paged over the Spectrum ROM by watching opcode fetches. The
EEPROM image, normally esxDOS, comes from the user and is never part of this
crate.

It implements the `Peripheral` trait and its `MemoryIntercept` from `emu198x-bus-sinclair-zx-spectrum`,
so any Spectrum-family machine that owns one calls its hooks from its own bus
handler. It depends on no machine.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
