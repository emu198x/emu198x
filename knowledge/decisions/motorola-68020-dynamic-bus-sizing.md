# Decision: preserve MC68020/MC68030 logical transfers across DSACK phases

**Date:** 2026-07-27
**Status:** implemented

## Question

How should the MC68020/MC68030 bus interface represent a byte, word or
longword whose physical phases depend on address alignment and the responder's
data-port width, without changing existing MC68000-shaped machine buses?

## Decision

The CPU retains one serialized logical data transfer while exposing each
physical phase through the existing pin-level `State::BusCycle` boundary.
Additional output state carries:

- the current SIZ value, represented as the number of operand bytes remaining;
- the physical D31-D0 write-data image;
- the original write operand or partial read accumulator.

The machine completes an evidence-backed phase with
`BusStatus::ReadySized { data, port }`. `port` is the DSACK-decoded byte, word
or longword responder width. The CPU determines how many sequential bytes fit
before that port's next boundary:

```text
accepted = min(bytes_remaining, port_bytes - (address mod port_bytes))
```

It then advances the phase address, updates SIZ and reissues the cycle when
bytes remain. Responder width is sampled again on every phase. This allows one
logical transfer to cross a device boundary or encounter different responder
widths without pre-decomposing it inside the instruction decoder.

Ordinary longword effective-address reads and writes use one logical
`ReadLong` or `WriteLong` micro-operation on the MC68020 and MC68030. Existing
staged stack and exception continuations retain their high-word/low-word
micro-operations in this slice. Each component word still participates in
dynamic sizing, but its continuation boundary is unchanged.

## Compatibility boundary

`BusStatus::Ready(u16)` retains its previous meaning: complete the currently
advertised abstract byte or word chunk. It does not imply a 16-bit DSACK
response.

This preserves existing MC68000, MC68010, machine and corpus harnesses. A
logical longword presented to such a responder completes as two compatibility
word phases at the same addresses used before this change. Only
`ReadySized` invokes alignment- and width-dependent phase reduction.

The new `BusStatus` variant and logical-long micro-operations are appended
after their pre-existing enum variants. Existing postcard discriminants
therefore remain stable.

Instruction prefetch and interrupt acknowledge remain on the compatibility
path. Cache-line fill, burst, retry and processor-specific fetch protocols are
separate work.

## Family scope

The MC68020 installs the dynamic-sizing capability. The MC68030 inherits it.

The MC68040 explicitly disables this inherited capability. It shares the
instruction implementation through the variant wrapper chain, but its external
bus protocol is not the MC68020/MC68030 SIZ/DSACK interface. MC68040 transfer
modelling requires its own boundary.

## A1200 scope

The A1200 initially returns a 32-bit sized response only for chip RAM. Alice
and the four DRAM byte lanes provide evidence for that width. Chip arbitration
is applied separately to every physical phase.

The following accesses deliberately retain compatibility dispatch:

- low-memory reads while OVL selects ROM;
- ROM at its normal window, because A1200 assemblies exist with both 16- and
  32-bit ROM populations and machine configuration does not yet record which
  is installed;
- CIA, custom-register, Gayle, RTC and other MMIO accesses whose lane or
  external response behaviour is not yet fully represented;
- unmapped cycles, which do not acquire an invented responder.

Configured Zorro-II memory is the next safe width migration at 16 bits. It is
not included in the initial chip-RAM slice.

The existing Amiga memory model retains a 16-bit floating-bus diagnostic
latch. Sized chip-RAM phases project the accepted value into that latch as a
compatibility observation. This decision does not define exact 32-bit A1200
open-bus residue.

## Write and read lanes

Writes expose the processor's duplicated D31-D0 pattern before the responder
width is known. A machine commits only the sequential lanes accepted by that
phase. Reads are supplied as a physical D31-D0 image, then selected by the CPU
from the current address and responder width.

This keeps device side effects in the machine layer. A wait does not commit a
phase, and holding `ReadySized` until the CPU samples it does not repeat a
write.

## Fault and snapshot boundaries

A terminal bus error records the current physical phase address. Bytes
committed by earlier phases remain committed. Exact Format `$A` restart state
and the BERR-plus-HALT retry handshake are not represented by this decision.

The logical transfer, read accumulator, SIZ value and physical write output are
serialized. Amiga runtime snapshots advance from schema version 22 to 23 and
reject version 22 before decoding its positional payload.

## Verification

Directed common-bus tests pin:

- SIZ and DSACK encodings;
- the complete size, alignment and responder-width phase-count matrix;
- read-lane placement and extraction;
- the manual's write-data duplication patterns.

MC68020 integration tests execute longword reads and writes at all four
alignments through fixed 8-, 16- and 32-bit responders. They verify phase
addresses, SIZ transitions, stored data and guard bytes. A mixed-width transfer
is serialized after its first phase and must continue tick-for-tick identically
after restore.

A1200 tests verify that an aligned chip-RAM longword receives one 32-bit
response, a held response does not repeat memory or diagnostic side effects,
a transfer at the end of chip RAM does not cross into `$200000`, and an
OVL-selected read remains on the ROM compatibility path. Existing MC68020
timing and unaligned-data tests remain unchanged and continue to pass through
`Ready(u16)`.

## Evidence basis

The MC68020 User's Manual defines DSACK port encodings in Table 5-1, SIZ
encodings in Table 5-2, data sizing and operand transfers in Tables 5-4 and
5-5, and the alignment/port phase counts in Table 5-6.

The Commodore A1200 schematics show the 32-bit CPU-to-Alice/chip-RAM path and
alternative 16-bit and 32-bit ROM population options.

## Related Documents

- [CPU bus interface](cpu-bus-interface.md)
- [MC68020 unaligned data access](motorola-68020-unaligned-data-access.md)
- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md)
- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
