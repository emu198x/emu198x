# emu198x-snk-lspc2

The SNK LSPC2 video chip and its palette RAM: the Neo Geo's display.

The Neo Geo has no tilemap background. Everything that moves is a sprite: 381
vertical strips, each 16 pixels wide and up to 32 tiles tall, that chain side
by side into larger objects and shrink in both directions. Over them sits the
fix layer, a 40×32 grid of 8×8 tiles for text. A line shows at most 96
sprites.

This crate holds video RAM and its auto-incrementing port, the sprite
attribute tables, auto-animation, the line counter, the vertical-blank and
display-position interrupts, and both banks of palette RAM with the dark bit
and the board's shadow line. Colours go through the board's resistor DACs to
ARGB.

## How it is modelled

`Lspc2::tick` is one 24 MHz master clock. The interrupts land on the master
clock they land on in hardware. The picture is built a line at a time: at
the start of each line the chip finishes the line before, then parses the
sprite table for the new line into one of the two sprite lists in video RAM.
Finishing a line draws the sprites that list names, then the fix layer, from
video RAM as it stands at that moment. Neo Geo games change video RAM
between lines, not within them, so a line is the granularity they observe.

Sprite tiles come from the cartridge's C-ROMs, fix tiles from its S-ROM or the
BIOS's SFIX, and vertical shrink from the board's L0 ROM. All three are
borrowed for each tick through `VideoRoms`: the graphics buses belong to the
video hardware alone, so a slice answers every fetch as the bus would.
Without an L0 ROM, vertical shrink uses a computed table that matches the ROM
at full size and approximates it when shrunk.

The facts follow MAME's Neo Geo driver (BSD-3-Clause).

## How it is tested

The unit tests build sprite and fix tiles in synthetic ROM images and check
the drawn pixels: tiles down a strip, both flips, chaining, every horizontal
shrink value, vertical shrink through a table, sprites taller than 32 tiles,
wrap-around at the left edge, the 96-sprite limit, draw order under the fix
layer and auto-animation. Others pin the video RAM port, the line counter,
the interrupt timing and the palette levels.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores. This crate is published so
siblings and outside projects can use the chip on its own. It is versioned
independently of the Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
