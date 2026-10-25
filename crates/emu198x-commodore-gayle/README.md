# emu198x-commodore-gayle

Commodore's Gayle gate array of the A600 and A1200: IDE task-file and control registers.

The chip exposes its state as public fields and is stepped by the machine on
the master clock, so it owns no bus and makes no callbacks: the machine reads
and drives its pins between ticks, the same as every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
