# Decision: SG-1000 interrupt acknowledge leaves the external bus high

**Date:** 2026-08-30  
**Status:** Active. Closes #226.

## Question

The SG-1000 machine supplies `$FF` to the Z80 during an interrupt-acknowledge
cycle. Could a cartridge drive another byte and make that assumption wrong?

## Evidence

Two independent reference implementations agree on `$FF`, by different paths:

- **Ares.** The SG-1000 CPU calls `irq()` without an external-bus argument
  (`emulators/multi-system/ares/ares/sg/cpu/cpu.cpp:33-36`). The Z80 interface
  defaults that argument to `$FF` (`component/processor/z80/z80.hpp:27`), and
  the IM 1 implementation explicitly forces `$FF` before executing the
  interrupt instruction (`z80.cpp:48-58`).
- **MAME.** The SG-1000 configuration connects the TMS9918A interrupt output
  directly to Z80 `IRQ0` and installs no daisy-chain or driver acknowledge
  callback (`emulators/multi-system/mame/src/mame/sega/sg1000.cpp:671-680`).
  MAME's Z80 therefore uses its declared default interrupt vector, `$FF`
  (`src/devices/cpu/z80/z80.h:60-65`).

The cartridge is mapped in the Z80 memory space, not as an interrupt-vector
device. Neither reference gives it an interrupt-acknowledge callback. This is
the important boundary: ordinary cartridge reads do not imply that the cart
drives the bus during the distinct `/M1` + `/IORQ` acknowledge cycle.

## Decision

Keep `machine-sega-sg-1000`'s `BusOp::IntAck` input at `$FF`.

For the shipped IM 1 path, the Z80 selects `RST $38` regardless of external
bus data; `$FF` is still the truthful undriven-bus value. If a future expansion
adds real vectored interrupt hardware, that device must own the acknowledge
cycle explicitly rather than routing the read through cartridge memory.

## Consequences

- #226 is a confirmed-correct implementation, not a latent defect.
- No emulation or snapshot change is required.
- IM 0 support in the shared Z80 remains a separate CPU-core question. It does
  not make an SG-1000 cartridge into an interrupt-vector device.

