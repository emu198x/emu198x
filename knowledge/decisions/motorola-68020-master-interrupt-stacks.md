# Decision: model MC68020 master-mode interrupts as paired stack frames

**Date:** 2026-07-25
**Status:** implemented

## Question

How does an MC68020 interrupt use MSP and ISP when the saved status
register has the M bit set, and how must `RTE` return through the
resulting frames?

## Decision

The shared register file selects A7 from all three architectural stack
pointers:

- user mode (`S=0`) selects USP, regardless of M;
- supervisor interrupt mode (`S=1, M=0`) selects ISP, represented by the
  existing `ssp` field;
- supervisor master mode (`S=1, M=1`) selects MSP.

This selection is enabled only by the MC68020-family wrappers. The
MC68000 and MC68010 leave the capability disabled, so their reserved
status-register bit 12 cannot redirect A7.

An interrupt accepted with saved M clear creates one ordinary Format
`$0` frame on ISP. An interrupt accepted with saved M set creates two
four-word, eight-byte frames:

1. the ordinary Format `$0` frame is written to MSP with the original
   saved SR, interrupted PC and acknowledged vector offset;
2. live M is cleared, selecting ISP;
3. a Format `$1` throwaway frame is written to ISP with the same PC and
   vector offset;
4. the throwaway frame's saved SR is the original saved SR with S forced
   set, while M remains set.

The handler therefore starts in supervisor interrupt mode (`S=1,
M=0`). Saved M, rather than saved S, selects the paired-frame path. A
user context can retain M set; in that case the master frame preserves
`S=0, M=1`, while the throwaway copy has `S=1, M=1`.

## Return sequence

`RTE` first consumes the Format `$1` frame from ISP. It advances ISP by
eight bytes, restores the throwaway SR and restarts `RTE` processing
instead of resuming at the throwaway PC. Restoring `S=1, M=1` selects
MSP, so the restarted operation consumes the ordinary frame there and
restores the interrupted SR and PC.

That MSP transition describes the interrupt-generated pair. Format `$1`
itself restarts according to the SR it restores; a constructed frame can
therefore redirect the second `RTE` pass to USP, ISP or MSP.

The implementation records which stack supplies the current frame. It
does not redirect pointer updates merely because the saved SR
has been read. This is required both for an ordinary return that changes
privilege level and for the Format `$1` restart that deliberately changes
stack banks.

## Family scope

`Cpu68030` inherits this behaviour through the wrapper chain.
`Cpu68040` currently inherits it as a compatibility path, but that does
not establish exact MC68040 interrupt-frame SR contents; its
processor-specific exception work must verify those semantics alongside
the deferred Format `$7` access-error frame.

The available MC68040 User's Manual is internally inconsistent at this
point. Section 8.2.9 describes the throwaway saved SR with M clear and
the new interrupt mask, while section 8.4.2 says the usual Format `$1`
return reads S and M set so the restarted `RTE` selects MSP. The latter
is operationally consistent with the paired-frame return and with the
MC68020 rule implemented here. Until an erratum or independent
MC68040-specific primary source resolves the conflict, the inherited SR
image remains a compatibility interpretation.

The inherited path is stack and frame behaviour, not a claim that the
processors expose identical external bus sequences.

The current path implements the ordinary Format `$0` master frame.
Coprocessor mid-instruction Format `$9` frames and the remaining long
fault-frame work retain their separate implementation status.

## Bus-sequence boundary

The MC68020 user's manual states that exception vector acquisition,
stacking and the other individual exception bus cycles are not
guaranteed to occur in the descriptive order. Addresses and
stack-pointer offsets are guaranteed.

The implementation therefore pins the selected stack, final frame
contents, live SR state and `RTE` transition. It does not present its
micro-operation order as a literal MC68020, MC68030 or MC68040 pin trace.

## Snapshot compatibility

Master-mode entry and return contain state that cannot be reconstructed
from live SR alone:

- whether the normal MSP frame has completed and the ISP throwaway frame
  is still pending;
- the SR and PC read from an in-flight `RTE`;
- which USP/ISP/MSP bank supplies the current `RTE` frame.

Those values are serialized. Amiga runtime snapshots therefore advance
from schema version 21 to 22 and reject version 21 before decoding the
positional postcard payload. The wrapper-only stack capability remains
non-serialized and is reinstalled after deserialization.

## Verification

Directed MC68020 regressions verify:

- A7 selection across USP, ISP and MSP;
- one Format `$0` frame when M is clear;
- paired Format `$0` and Format `$1` frames when M is set;
- a user-mode interrupt with saved M set preserving S clear only in the
  master frame;
- handler execution with M clear;
- `RTE` consuming ISP and then MSP before restoring both pointers;
- a constructed Format `$1` frame restarting on USP;
- deterministic serialization during the transition from the MSP frame
  to the ISP frame;
- deterministic serialization at the Format `$1` `RTE` restart.

Directed MC68030 and MC68040 regressions verify only that construction
and deserialization retain the inherited stack-selection capability.
They are not evidence of exact MC68040 interrupt-entry parity. The
composed A1200 snapshot suite verifies the same boundary around its
nested MC68020.

## Evidence basis

The MC68020 User's Manual defines supervisor stack selection in section
2.1.1, the paired interrupt frames in section 6.1.9, and the Format `$1`
restart in section 6.1.12. Section 6.1 also states the exception
bus-cycle ordering boundary.

## Related Documents

- [MC68010+ acknowledged interrupt vectors](motorola-68010-acknowledged-interrupt-vector.md)
- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md)
- [Motorola 68k variant pattern](motorola-68k-variant-pattern.md)
- [M68k test-oracle strategy](m68k-test-oracle-strategy.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
- [CPU bus interface](cpu-bus-interface.md)
