# emu198x-commodore-amiga-autoconfig

The Amiga's Zorro II autoconfig protocol: the state machine an expansion
board runs so that `expansion.library` can find it at boot and map it. It
covers probe-window reads of the board's config ROM, with its split nibbles
and inverted bits, the base-address writes that map the board, the shut-up
escape, and RAM behind the board once it is configured.

It ships one board type, fast RAM, which is enough to give Exec Zorro II
memory. The host forwards the reads and writes that fall in the probe window
and the board's mapped range; the crate owns no bus and makes no callbacks.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
