# emu198x-motorola-mfp-68901

The Motorola MC68901 multi-function peripheral, as used in the Atari ST:
eight general-purpose I/O lines with edge interrupts, four timers in delay,
event-count and pulse-width modes, a 16-channel vectored interrupt
controller, and a USART clocked from outside.

The chip is clocked by its host: one `tick()` is one cycle of the MFP's own
clock. Register access is `read()` and `write()` with the register number,
the interrupt request line is `irq()`, and `acknowledge()` is the
interrupt acknowledge cycle. The chip owns no bus and makes no callbacks.

Facts come from the MC68901 data sheet and are cross-checked against
Hatari's `mfp.c` (GPL-2.0), which was not used as a code source.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
