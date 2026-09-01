# Probe Source

This directory contains the original 68000 source for the OCS Copper colour
phase probe.

`bootblock.S` is a minimal Amiga boot-block loader. It reuses the boot
device's open I/O request to load the sector-aligned payload at `0x00030000`,
checks the synchronous read result, and transfers control.

`probe.S` disables DMA and CPU interrupts, installs one low-resolution
bitplane and a Copper list, publishes a ready record at `0x0002ff00`, and
increments its field counter from the vertical-blank request. The bitplane
contains one set bit per row. Its white `COLOR01` pixel is an output anchor
which is independent of `COLOR00`.

On beam lines 128 through 135, the Copper restores the dark-blue guard at
horizontal CCK 32. At horizontal CCK 144 it executes four `COLOR00` MOVEs
back-to-back: red, green, blue, and yellow. There is no WAIT or other Copper
instruction between those MOVEs. The shortened bitplane fetch ends before
the colour sequence, avoiding bitplane-DMA stalls between adjacent MOVEs.
Beam line 132 is the declared measurement row.

The probe uses only OCS registers after the Kickstart boot loader transfers
control. It does not depend on Kickstart libraries or a filesystem.

`custom-registers.inc` contains only the addresses and ready-record fields
required by the two sources. `case.inc` is generated in a temporary build
directory from the validated case record; it is not a source file.

All source in this directory is original and carries an SPDX `CC0-1.0`
identifier.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the input record.
- [`../tools/README.md`](../tools/README.md) defines the build.
