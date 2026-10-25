# emu198x-tape-playback

Machine-agnostic tape timing spans and their player: every cassette format reduced to "hold this level for N T-states".

It holds the span model and the player, and knows nothing about any system or format. Span durations are in the T-states of whoever loaded them; converting a format's reference clock to a machine's is the loader's job, before the spans reach the player.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
