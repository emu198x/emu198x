# emu198x-commodore-denise-aga

Commodore's Lisa, the AGA Denise, built on the ECS Super Denise: the 256-entry
24-bit palette, HAM8, BPLCON4's bitplane XOR and sprite bank, FMODE-driven
16-, 32- and 64-pixel sprites, Lisa's programmable horizontal blanking and the
DENISEID that tells Kickstart 3.x it is on an AGA machine.

Lisa implements `DeniseChip` from `emu198x-commodore-denise-ocs`, so a board
written against that trait drives OCS Denise, ECS Super Denise and Lisa alike.
The chip owns no bus and makes no callbacks: the board hands it register
writes, bitplane and sprite data and timing strobes between ticks, the same as
every Emu198x chip.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
