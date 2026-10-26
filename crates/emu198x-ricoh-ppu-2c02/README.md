# emu198x-ricoh-ppu-2c02

The Ricoh 2C02, the NES and Famicom PPU, stepped one dot at a time. It covers
the background tile pipeline, sprite evaluation with the hardware overflow bug,
the loopy scroll registers, NMI timing with its two-dot pipeline delay, the
odd-frame dot skip and PPU open-bus decay, with the PAL 2C07's timing as a
region setting.

The machine drives the PPU's register port (`$2000-$2007`) and routes the
public `nmi` field to the CPU between ticks. The PPU reaches the cartridge
through `PpuCartridge`, a trait this crate owns: CHR reads and writes, the
nametable `Mirroring` mode, nametable overrides, and the A12 edges that MMC3's
IRQ counter counts. Any cartridge model can implement it; this crate needs no
particular one.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use it on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.
