# emu198x-nintendo-game-boy-ppu

The Game Boy's PPU, the DMG's and the Game Boy Color's: a pixel FIFO ticked
once per dot, with the background and window fetcher, object fetch stalls,
SCX fine-scroll discard and OAM scan each reading the registers on the dot the
hardware does. It renders 160×144 2-bit shades, and on a CGB RGB555 colours
through its eight BG and eight OBJ palettes.

The machine steps the chip once per dot and hands it VRAM and OAM as slices,
so the PPU owns no bus and makes no callbacks; the machine reads and writes
its registers between ticks, the same as every Emu198x chip. VBlank and STAT
interrupts are one-shot pulses the machine consumes and ORs into `IF`.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
