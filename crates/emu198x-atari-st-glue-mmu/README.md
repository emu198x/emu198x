# emu198x-atari-st-glue-mmu

Two of the Atari ST's custom chips, which share the work of putting a picture
on the screen and RAM on the bus:

- **GLUE** generates the video timing: 50, 60 and 71 Hz line and frame
  lengths, sync, the display-enable window, and the HBL and VBL interrupts.
- **MMU** owns the RAM: the memory configuration register and its DRAM row
  and column mapping, the video base and counter with one fetch every four
  CPU cycles, and the DMA address counter.

`tick()` is one 8 MHz CPU cycle. Between ticks the host reads the
display-enable line, takes any video fetch that is due and the HBL and VBL
pulses. The chips own no bus and make no callbacks.

Facts come from the Atari ST documentation and are cross-checked against
Hatari's `video.c` and `stMemory.c` (GPL-2.0), which were not used as a code
source. The DRAM row and column rule is Christian Zietz's reverse-engineering
of the MMU.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
