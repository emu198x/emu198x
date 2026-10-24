# emu198x-ricoh-rp5c01

The Ricoh RP5C01 real-time clock, the MSX2's clock chip: a BCD time and
calendar, an alarm block, and two blocks of battery-backed RAM where the
MSX2 BIOS keeps its screen settings and password.

The chip is a register file of four-bit nibbles. It owns no bus and makes no
callbacks: the machine writes its register latch and data port, and steps it
at the chip's 16,384 Hz internal rate. Time therefore passes with emulated
time, not the host's clock, so a run is deterministic and a snapshot resumes
on the second it was taken.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. The register masks, the 12/24-hour encoding
and the test-mode carries are facts read from openMSX (GPL-2.0), not code
taken from it.

Licensed GPL-2.0-or-later.
