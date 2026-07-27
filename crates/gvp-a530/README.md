# GVP A530

## Purpose

`gvp-a530` preserves the board-local configuration and memory state needed to
integrate a GVP A530 accelerator into an Amiga machine.

## Scope

The crate models the documented 1, 2, 4 and 8 MiB local-RAM configurations,
the factory cache-enable and autoboot jumper states, a Zorro-II Autoconfig
memory function, and the accelerator's full 32-bit local-RAM access path.

The A530 manual identifies a 40 MHz MC68EC030 and a shipped minimum of 1 MiB
RAM. It also records factory J3 cache enabled and J9 autoboot enabled. These
flags are configuration facts here; the crate does not implement the CPU
cache or autoboot behaviour.

The memory-function manufacturer/product pair `2017/9` is taken from WinUAE.
It is a secondary-oracle compatibility identity because the current primary
manual evidence does not establish those values.

## Relationship to neighbouring crates

`commodore-amiga-autoconfig` owns the generic Zorro-II probe and mapping state
machine. `motorola-68k-common` supplies the MC68020/MC68030 transfer-size and
32-bit lane helpers used by the local-RAM path. The Amiga machine integration owns
the processor, rational clocking, cache-disable wiring and the synchronized
16-bit motherboard bridge.

## Expected contents

This crate contains:

- static A530 RAM and jumper configuration;
- serializable Autoconfig memory-function state;
- mapped byte and full 32-bit sized local-RAM access; and
- directed identity, mapping, byte-order and save-state tests.

It does not contain SCSI registers, controller firmware, disk media, a boot
ROM, CPU ownership, CPU execution, motherboard arbitration or invented
autoboot behaviour.
