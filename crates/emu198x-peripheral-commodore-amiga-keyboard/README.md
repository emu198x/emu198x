# emu198x-peripheral-commodore-amiga-keyboard

The Amiga keyboard as the host sees it. The real keyboard has a 6500/1
microcontroller that scans the keys and talks to the host serially over
CIA-A's SP and CNT lines; this crate models that controller's behaviour at
the protocol level, not its program. It covers the power-up sequence, the
rotated and inverted keycodes, the host handshake, and the timeout and
resynchronisation that follow a missed one, producing bytes at E-clock rate.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
