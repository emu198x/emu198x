# Probe Source

This directory contains the original 68000 source shared by every case.

`bootblock.S` is a minimal Amiga boot-block loader. It reuses the boot
device's open I/O request to load the sector-aligned payload at
`0x00030000`, checks the synchronous read result, and transfers control.

`probe.S` disables DMA and CPU interrupts, opens `BPLCON0.ECSENA` while
writing `BPLCON3`, restores the case-defined steady-state controls, publishes
a ready record at `0x0002ff00`, and increments its field counter from the
vertical-blank request. The temporary access write lets a case preload an
enhanced-display control before deliberately clearing `ECSENA`. The probe
does not use Kickstart libraries after loading and does not depend on a
filesystem.

`custom-registers.inc` contains only the register addresses and masks required
by those two sources. `case.inc` is generated in a temporary build directory
from the validated case record; it is not a source file.

The sources intentionally render no bitplanes. A non-black `COLOR00` provides
a stable signal across the border and display area so retained blanking can be
observed without confusing it with image data.

All source in this directory is original and carries an SPDX
`CC0-1.0` identifier.

## Related files

- [`../cases/README.md`](../cases/README.md) defines the case inputs.
- [`../tools/README.md`](../tools/README.md) defines the build.
