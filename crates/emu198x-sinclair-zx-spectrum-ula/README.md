# emu198x-sinclair-zx-spectrum-ula

The video engine every ZX Spectrum-family ULA shares. It covers the screen and
attribute fetch, the two-stage pixel shifter, the border latch, FLASH, the
interrupt window, the floating bus and snow. Each variant configures it with
its own timing and builds its ULA around it. The variants are the Ferranti
6C001E (48K), Sinclair 7K010E (128K), Amstrad 40077 (+2A/+3), Timex SCLD,
Pentagon and Scorpion. Contention and port decoding differ between them, so
they stay in each variant's own crate.

The engine renders into a palette-indexed `u8` framebuffer. It reads the
screen through `MemoryBus` from `emu198x-bus-sinclair-zx-spectrum`, which also
defines the `Ula` trait the variants implement.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
