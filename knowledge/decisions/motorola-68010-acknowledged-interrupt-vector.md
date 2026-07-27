# Decision: use the acknowledged vector throughout MC68010+ interrupt entry

**Date:** 2026-07-25
**Status:** implemented

## Question

Which vector supplies the Format/Vector word and handler fetch when an
MC68010-or-later interrupt acknowledge returns a device vector,
autovector or spurious response?

## Decision

The interrupt-acknowledge result is the single selected vector for the
remainder of exception entry.

On processors with formatted exception frames, the CPU completes
acknowledge before constructing the Format/Vector word. It retains the
selected vector in `exc_vector`, stacks `vector * 4` in the low 12 bits
of a Format `$0` word, and uses the same vector for the handler fetch.
The retained value also prevents the shared continuation from issuing a
second acknowledge cycle after stacking.

This rule covers every response represented by the current bus boundary:

- `BusStatus::Ready(vector)` uses the supplied vector, including vector
  15 for an uninitialized peripheral;
- an Amiga collapsed autovector response uses `24 + accepted_level`;
- `BusStatus::Error` during acknowledge uses spurious vector 24.

The original MC68000 path is unchanged. Its ordinary interrupt frame has
no Format/Vector word, and it continues to stack PC and SR before the
acknowledge cycle in the existing implementation.

## Why the accepted level is insufficient

The accepted interrupt level determines the active SR mask and the
autovector number. It does not determine a device-supplied vector or the
spurious response.

The previous formatted-frame path prepared `(24 + level) * 4` before
acknowledge. A device vector could therefore fetch one handler while RTE
observed a frame naming another vector. BERR had the same mismatch:
handler 24 with an autovector-derived offset. Using the acknowledge
result for both consumers removes that split identity.

## Family scope

`Cpu68010`, `Cpu68020`, `Cpu68030` and `Cpu68040` inherit this
architectural rule through the shared exception continuation. The rule
does not make their external acknowledge buses identical.

The current compatibility surface represents acknowledge as
`FunctionCode::InterruptAck` plus a 68000-shaped address and a collapsed
`BusStatus` response. It does not yet model:

- the full CPU-space address presentation of the MC68020 and MC68030;
- the MC68040 transfer-type, transfer-modifier and termination signals;
- BERR-plus-HALT retry;
- MC68020-and-later master-stack and Format `$1` throwaway frames.

Those are separate variant capabilities. They must not be inferred from
the now-correct vector/frame identity.

## Bus-sequence boundary

This change is architecturally ordered around acknowledge: it obtains the
selected vector before writing the Format/Vector word. It is not a
cycle-exact model of every stack write.

Motorola's family bus timing diagram shows a PCL stack cycle before
interrupt acknowledge, followed by the remaining stack and vector-fetch
cycles. The shared micro-operation path currently performs acknowledge
before all formatted-frame writes. Reproducing the literal sequence
requires a dedicated exception-stack continuation and variant bus-width
rules. That work is deliberately separate so final-frame correctness is
not coupled to an inaccurate claim about later-family pins.

## Snapshot compatibility

The existing `exc_vector` field is sufficient for the acknowledged
vector. Its interrupt-entry meaning is new, and follow-up tag 46 becomes
reachable between acknowledge and Format/Vector stacking.

The pending frame PC in `exc_pending_pc` must also be serialized. It was
previously skipped even though formatted synchronous exceptions already
used it between Format/Vector and PC stacking. Restoring at that boundary
could therefore substitute zero for the saved PC.

Amiga runtime snapshots therefore advance from schema version 20 to 21.
A version-20 payload lacks the pending PC, and a version-20 reader does
not recognize the new continuation tag. The runtime rejects version 20
instead of guessing either part of an in-flight exception.

## Verification

The MC68010 regression matrix uses a nonzero VBR and verifies:

- supplied vector 64 fetches handler 64 and stacks `$0100`;
- the collapsed level-3 autovector fetches handler 27 and stacks `$006C`;
- BERR fetches spurious handler 24 and stacks `$0060`;
- supplied vector 15 remains distinct and stacks `$003C`;
- every case creates an eight-byte frame containing the saved SR and PC;
- a serialized post-acknowledge continuation retains the pending PC and
  resumes identically.

Inheritance regressions apply the supplied-vector case to the MC68020,
MC68030 and MC68040 wrappers. The MC68040 test covers the shared
compatibility semantics only, not its physical acknowledge protocol.

The composed A1200 runtime is also captured at the exact post-acknowledge,
pre-frame boundary. Restore preserves its nested `Cpu68020`, round-trips
byte-identically, completes deterministically and stacks the level-3
autovector offset `$006C`.

## Evidence basis

The MC68010 exception description obtains the interrupt vector through
acknowledge and stores that vector multiplied by four in the
Format/Vector word. The canonical rules are recorded in
[MC68010 Exception Vectoring](../../../../codex/05-specifications/processors/m68010/exception-vectoring.md)
and
[MC68010 Exception Stack Frames](../../../../codex/05-specifications/processors/m68010/exception-stack-frames.md),
based on sections 6.2.5 and 6.3.2 of Motorola's ninth-edition family
user's manual.

## Related Documents

- [Accepted MC68000 interrupt level](motorola-68000-interrupt-acknowledge-level.md)
- [MC68000 spurious interrupt response](motorola-68000-spurious-interrupt-response.md)
- [MC68000 level-7 transition recognition](motorola-68000-level-7-transition.md)
- [CPU bus interface](cpu-bus-interface.md)
- [68k variant pattern](motorola-68k-variant-pattern.md)
- [Save-state live-machine serde](savestate-live-machine-serde.md)
