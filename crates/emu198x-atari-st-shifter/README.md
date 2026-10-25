# emu198x-atari-st-shifter

The Atari ST Shifter: the 16-entry palette, the resolution register, and the
shift registers that turn the bitplane words the MMU fetches into 320×200
16-colour, 640×200 4-colour and 640×400 monochrome pixels.

The host passes each fetched video word to `load()` and calls `tick()` once
per CPU cycle with the display-enable line; each tick gives one, two or four
pixels. The chip owns no bus and makes no callbacks: the machine forwards its
register reads and writes and the MMU's fetches.

Facts come from the Atari ST documentation and are cross-checked against
Hatari's `video.c` (GPL-2.0), which was not used as a code source.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
