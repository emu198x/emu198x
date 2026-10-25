# emu198x-motorola-acia-6850

The Motorola MC6850 asynchronous communications interface adapter, as used
twice in the Atari ST for the keyboard and MIDI links: control and status
registers, the clock divider and word formats, transmit and receive framing,
overrun, and the interrupt request.

The chip is clocked by its host: one `tick()` is one edge of the transmit
and receive clock input. Bytes from the far end of the line arrive through
`receive()` and bytes the chip sends come out of `take_transmitted()`, each
once its frame has been clocked through. The chip owns no bus and makes no
callbacks.

Facts come from the MC6850 data sheet and are cross-checked against Hatari's
`acia.c` (GPL-2.0), which was not used as a code source.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
