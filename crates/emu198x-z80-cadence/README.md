# emu198x-z80-cadence

The cadence every Z80 machine shares: two CPU half-cycles per machine T-state, with the interrupt pins fed before each.

A machine that owns an `emu198x-zilog-z80` implements the hooks of `Z80Machine` in the order its hardware runs them and gets `advance_tstates` and `tick_one_halfcycle` in return. The crate owns no frame loop and schedules no chips: how a frame is paced, and how other chips are clocked, stays with the machine. It depends on nothing, not even the CPU crate.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
