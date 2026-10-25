# emu198x-peripheral-commodore-amiga-floppy

The Amiga floppy drive mechanism: head stepping, the motor, disk change
detection, the drive ID, and MFM track encoding of an ADF image. Control
signals come from CIA-B port B and status signals go back to CIA-A port A;
the machine moves those pins between ticks.

Disk images are read with
[`format198x-commodore-amiga-adf`](https://crates.io/crates/format198x-commodore-amiga-adf).
Writes change the image in memory only, never the file it came from.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
