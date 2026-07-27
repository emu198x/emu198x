# Decision: install MC68030 cache control before the data-cache datapath

**Status:** Implemented, with data-cache and burst behaviour deferred

## Question

How should the MC68030 expose its cache-control register and external
cache-disable signal while the emulator models only the inherited instruction
cache?

## Evidence

MC68030 User's Manual sections 5.11.1 and 6.3 define:

- CACR bits 4–0 for the instruction cache and bits 13–8 for the data cache;
- persistent bits WA, DBE, FD, ED, IBE, FI and EI;
- momentary clear commands CD, CED, CI and CEI, which always read zero;
- reserved bits which ignore writes and read zero; and
- active-low CDIS, which disables both caches without flushing them.

This gives a defined-bit mask of `$00003F1F`, a persistent mask of
`$00003313`, and a momentary-command mask of `$00000C0C`.

The bundled Musashi oracle instead retains clear-command bits and additional
reserved bits in its MC68030 CACR value. That behaviour conflicts with the
processor manual. The generated corpus remains useful for inherited
instruction behaviour, but does not override the manual for this directed
register test.

The MC68EC030 technical summary also establishes that the EC part retains the
external MC68881/MC68882 coprocessor interface. It removes the on-die MMU, not
the optional FPU interface. Whether a board fits a coprocessor remains machine
configuration.

## Decision

The shared MOVEC implementation consumes variant-installed CACR masks.
`Cpu68030` installs the MC68030 defined and momentary masks after construction
and deserialization. The MC68020 and current MC68040 compatibility paths
install their existing four-bit behaviour explicitly, so structural wrapper
inheritance cannot change their register semantics.

CI and CEI invalidate the implemented instruction cache. CD and CED are
accepted and read zero, but have no data-cache effect until that cache exists.
Persistent data-cache and burst controls are retained in CACR so software sees
the correct register even though their datapaths remain deferred.

`Cpu68030::set_cdis_asserted` exposes CDIS as an asserted logical input. While
asserted, the shared instruction-cache path suppresses hits and fills without
invalidating entries. The input is not serialized: it is combinational board
wiring and must be driven again by the owning machine after restore.

Reset clears CACR and invalidates the installed instruction cache.

## Consequences

An accelerator may hold CDIS asserted as an explicit first implementation
limit without lying about CACR readback or losing cached entries when that
input changes.

This slice does not model the MC68030 data cache, its exact instruction-cache
line/tag organization, instruction or data burst fills, write allocation,
CIIN/CIOUT, DMA coherency, or PMMU execution. The inherited MC68020
instruction-cache representation remains a conservative timing model rather
than a claim about the MC68030's four-longword line fills. A real data cache
will be architecturally observable across DMA and save states; it must be
serialized and introduced with the corresponding machine-bus protocol rather
than as an operand-sized read shortcut.

## Verification

Directed tests cover:

- all-ones CACR write and manual-defined readback;
- instruction-cache clear and selected-entry clear;
- data-clear commands leaving the instruction cache unchanged;
- CDIS hit suppression, fill suppression and retained-entry reuse;
- reset invalidation;
- deserialization restoring the MC68030 binding; and
- explicit MC68020 and MC68040 compatibility masks.

## Related Documents

- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md)
- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md)
- [M68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [MC68020/MC68030 dynamic bus sizing](motorola-68020-dynamic-bus-sizing.md)
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md)
