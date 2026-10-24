# emu198x-ti-tms9918

The Texas Instruments TMS9918A video display processor, as used by the ColecoVision, Memotech MTX, MSX, Sega SG-1000, Sord M5, Spectravideo and Tatung Einstein.

The chip exposes its state as public fields and is stepped by the machine on
the master clock, so it owns no bus and makes no callbacks: the machine reads
and drives its pins between ticks, the same as every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
