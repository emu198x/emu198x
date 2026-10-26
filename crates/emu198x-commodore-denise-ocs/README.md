# emu198x-commodore-denise-ocs

Commodore's OCS Denise: the Amiga's pixel shifter, playfield compositor and sprite engine.

The chip exposes its state as public fields and is stepped by the machine on
the master clock, so it owns no bus and makes no callbacks: the machine reads
and drives its pins between ticks, the same as every Emu198x chip.

The crate also defines `DeniseChip`, the surface a board drives on any Denise:
this OCS chip, the ECS Super Denise and the AGA Lisa each implement it in their
own crates, so one board can be written for all three.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
