# Probe Source

This directory contains the original 68000 source for the fixed-sprite probe.

`bootblock.S` is a minimal Amiga boot-block loader. It uses the open boot
device I/O request supplied by Kickstart to read the sector-aligned payload at
`0x00030000`, checks the synchronous read result, and transfers control. The
payload makes no operating-system or filesystem calls and does not require
Workbench.

`probe.S` disables DMA and interrupts before programming the display. It
creates one low-resolution bitplane, places a one-bit marker in every row,
points sprite channel 0 at a 16-line solid sprite, and points channels 1 to 7
at an empty sprite. It explicitly selects the 16-bit AGA fetch mode and the
OCS-compatible sprite palette mapping; older display chips ignore those
register writes.

Bitplane and sprite DMA pointers advance during a field. The field loop
restores all of them when the vertical-blank request arrives, before advancing
the ready counter. Captures taken after the settle interval therefore observe
the same source data in every field.

The fixed-width, big-endian record at `0x0002ff00` contains the `SPHX` magic,
field counter, programmed register words, source pointers, marker geometry,
sample line, and serial identity. The record reports inputs only; it does not
publish an expected output coordinate.

`custom-registers.inc` contains only the custom-register offsets, DMA masks,
and ready-record layout required by the probe. `case.inc` is generated in a
temporary build directory from the validated case record.

All source in this directory is original and carries an SPDX `CC0-1.0`
identifier.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the source inputs.
- [`../tools/README.md`](../tools/README.md) defines the build.
- [`../schema/README.md`](../schema/README.md) defines how captures report the
  ready record and visible edges.
