# emu198x-nintendo-s-ppu

The Super Nintendo's picture processor pair, S-PPU1 (5C77) and S-PPU2
(5C78), modelled as one chip because the console never separates them:
backgrounds in Modes 0 to 7 with offset-per-tile and Mode 7 matrix maths,
128 sprites with per-line evaluation and tile fetch, the two windows, colour
math between the main and sub screens, and the H and V counters with their
latches.

The host calls `tick()` once per dot, four master clocks. Each tick does the
work the hardware does at that point in the line, so a register written
between ticks splits the line where it was written, and VRAM, OAM and CGRAM
accesses land where the hardware would place them. The chip owns no bus and
makes no callbacks: the machine forwards the register reads and writes.

Checked frame by frame against [ares](https://github.com/ares-emulator/ares)'s
accurate PPU (ISC licence, © ares team, Near et al): a harness in
`tests/oracle/` compiles ares's own source against scripted cases.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
