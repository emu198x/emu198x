# Decision: permit MC68020-family data operands at odd addresses

**Date:** 2026-07-25
**Status:** implemented for logical RAM access

## Question

How should the shared 68k core distinguish the MC68000/MC68010
odd-address rule from MC68020-family data accesses before the external
bus models dynamic sizing and split cycles?

## Decision

The shared core has an MC68020-family unaligned-data capability.

When disabled, word and long-word transfers at odd addresses take the
existing address-error path. This remains the MC68000 and MC68010
behaviour.

When enabled, every data transfer may begin at any byte address,
including:

- ordinary word and long-word operand reads and writes;
- PC-relative data reads in program space;
- stack pushes and pops.

Instruction prefetch remains word-aligned. A `FetchIRC` operation at an
odd address still takes vector 3. The MC68020 and MC68030 use a Format
`$A` group-0 frame. `Cpu68040` currently inherits that compatibility
path, but MC68040 silicon uses Format `$7`; the processor-specific
Format `$7` path remains deferred.

For an odd next-instruction boundary, the Format `$A` common PC field
contains the rejected instruction address rather than the preceding
instruction's start address. This pins the architectural frame PC; it
does not imply that the remaining pipeline words or fault-rerun state
are complete.

`Cpu68020` installs the capability after construction and
deserialization; cloning preserves it. `Cpu68030` and `Cpu68040`
inherit it through the variant-wrapper chain.

## Current bus boundary

The current CPU/bus contract carries a byte or word transaction with an
exact start address. A long transfer is two word transactions at
`address` and `address + 2`. The Amiga RAM path composes each word from
the bytes at the requested address and the following address.

This is sufficient for the correct logical value of an unaligned RAM
operand and for exact byte placement on a RAM write.

It is not a complete MC68020 external-bus model. The current contract
does not expose:

- SIZ0/SIZ1 transfer-size signals;
- DSACK-selected responder width;
- byte-lane strobes;
- the extra split phases required by alignment and port width;
- the individual side effects of a split access spanning device
  registers.

Consequently, this decision does not claim cycle-accurate unaligned
timing or correct odd word/long access to memory-mapped devices. Those
require a separate dynamic-bus-sizing implementation, with responder
widths pinned per machine address region from primary hardware
evidence.

## Why the capability belongs in the shared core

Address rejection occurs before a bus cycle is exposed to the machine.
The distinction must therefore be made at that gate. Keeping the
default disabled preserves the MC68000 and MC68010 without duplicating
their memory pipelines, while the MC68020 wrapper installs only the
architectural delta.

The capability is configuration rather than live execution state. It
is skipped by serde and reinstalled by the wrapper, so this change does
not require another snapshot-schema revision.

## Verification

Directed tests verify:

- odd `MOVE.W` and `MOVE.L` RAM reads produce the expected big-endian
  values without vector 3;
- odd `MOVE.W` and `MOVE.L` RAM writes alter only the requested bytes;
- an odd `UNLK` long pop uses and advances MSP without disturbing ISP
  or USP;
- an odd jump target still takes vector 3 and builds the complete
  16-word Format `$A` footprint on MSP, with the rejected target in its
  common PC field;
- wrapper cloning and deserialization reinstall the capability;
- shared Amiga memory and the A1200 chip-RAM dispatch preserve exact
  odd transaction addresses.

The generated MC68020 and FPU corpus harnesses also preserve odd
addresses when composing fixture memory words.

## Evidence basis

The MC68020 User's Manual permits byte, word and long-word data operands
at any byte address. It retains the address error for an instruction or
extension-word prefetch at an odd address. Section 5.2.2 and Table 5-6
describe the additional bus cycles as a function of operand size,
address offset and responder width; those cycles are the deferred bus
work identified above.

## Related Documents

- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md)
- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md)
- [MC68020 master-mode interrupt stacks](motorola-68020-master-interrupt-stacks.md)
- [M68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [CPU bus interface](cpu-bus-interface.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
