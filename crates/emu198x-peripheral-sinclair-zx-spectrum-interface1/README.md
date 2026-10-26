# emu198x-peripheral-sinclair-zx-spectrum-interface1

Sinclair's ZX Interface 1 and up to eight ZX Microdrives: the 8 KB shadow
ROM, the Microdrive controller, and the RS-232 and ZX Net ports at register
level. The ROM image comes from the user and is never part of this crate.

A cartridge is a `MicrodriveTape`: the sectors on its tape loop, in tape
order, and its write-protect tab. That is the layout the drive needs, not a
file format. A host reads an `.mdr` file with a format crate and hands the
sectors over.

It implements the `Peripheral` trait and its `MemoryIntercept` from `emu198x-bus-sinclair-zx-spectrum`,
so any Spectrum-family machine that owns one calls its hooks from its own bus
handler. It depends on no machine.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
