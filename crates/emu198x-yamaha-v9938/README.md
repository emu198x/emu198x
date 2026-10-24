# emu198x-yamaha-v9938

The Yamaha V9938 video display processor, the MSX2's video chip: 128 KB of
VRAM, the TMS9918's four screen modes plus Text 2 and Graphic 3–7, a
512-colour palette, sprite mode 2, the line interrupt, vertical scroll and
the command engine.

The chip exposes its state through its ports and public fields and is
stepped by the machine one dot at a time, so it owns no bus and makes no
callbacks: the machine drives its ports between ticks, the same as every
Emu198x chip. What is approximate (command timing, CPU access slots,
interlace) is listed in the crate documentation.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. Register masks, the planar mapping, the
default palettes and the command delays are facts read from openMSX
(GPL-2.0), not code taken from it.

Licensed GPL-2.0-or-later.
