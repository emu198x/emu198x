# Decision: Copper WAIT and SKIP comparison phase

**Date:** July 2026

## The question

When does the Copper sample the beam and blitter-finished condition for
`WAIT` and `SKIP`?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed pages 20–21
and 30–32, defines the instruction formats, masked beam comparison and
aggregate instruction costs. It assigns two memory cycles to `MOVE` and
`SKIP`, three to `WAIT`, and applies the position-comparison rules to both
conditional instructions. Appendix A, printed page 274, states that BFD
controls the blitter-finished condition for both `WAIT` and `SKIP`.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` does not evaluate `SKIP` when
its second instruction word is decoded. It advances through
`COP_skip_in2` and `COP_skip1`, then calls the shared Copper comparator
from `COP_skip`. Its `WAIT` path likewise reaches the comparator after
instruction fetch.

The inspected Minimig revision
`3ab91cd9220d4d047886d215b515227cbe568bdd` routes both instructions from
`FETCH2` through `WAITSKIP1` and `WAITSKIP2`. The live beam and
blitter-busy inputs feed the comparison used in `WAITSKIP2`.

These implementations agree on the observable sampling boundary:
comparison follows instruction-pair fetch. Their internal state counts
do not map directly onto the current emulator's coarser eligible-CCK
abstraction or the manual's aggregate memory-cycle descriptions.

## The decision

Decoding either `WAIT` or `SKIP` arms a serialized pending comparison.
It records:

- the target;
- the position mask;
- BFD; and
- whether the instruction is `WAIT` or `SKIP`.

The instruction does not sample the beam or visible blitter-busy input
at decode.

On the next eligible modeled Copper decision CCK, the Copper evaluates
the stored position condition against the live beam and applies BFD to
the live externally visible blitter-busy signal.

For `WAIT`:

- a satisfied condition returns to normal instruction fetch;
- an unsatisfied condition enters the persistent waiting state; and
- the persistent state continues sampling the live condition until it
  is satisfied.

For `SKIP`:

- a satisfied condition advances the Copper program counter over the
  following instruction pair; and
- an unsatisfied condition resumes fetch at that following pair.

`SKIP` never enters the persistent `WAIT` state. `COPJMP1` and
`COPJMP2` clear any pending comparison as part of restarting the Copper.

This boundary matters when status changes after decode. In particular,
the first accepted A1000 blitter-startup CCK can make BBUSY visible
before a pending BFD-clear `SKIP` compares. Blitter completion can
produce the inverse transition.

## Save-state compatibility

The pending instruction kind is hidden execution state. It cannot be
reconstructed from the Copper program counter, stored condition or
current beam because `WAIT` and `SKIP` have the same first-word shape
and differ in a second-word bit that has already been consumed.

The Amiga runtime envelope schema version 16 serializes the pending
kind alongside the shared Agnus blitter-startup phase. Version 15 is
rejected before payload decoding. Raw machine and chip postcards remain
unversioned and change positional layout.

## Model boundary

The current Copper represents the post-fetch comparison as one pending
eligible decision CCK. This fixes which live inputs determine the
instruction without claiming that one field reproduces every internal
idle, request and wake state of a physical Agnus.

## Deferred behaviour

This decision does not define:

- the exact number and ownership of all internal idle or dummy cycles;
- comparison progress while a nominal Copper slot is occupied;
- the exact relationship between Copper wake-up, a yielded bus request
  and the following first-word fetch;
- same-CCK ordering at blitter completion; or
- undocumented revision differences.

Those questions require bus traces or a separately bounded
cycle-pipeline decision.

## Verification

Hermetic tests cover:

- BFD-clear `SKIP` decoding while BBUSY is clear and comparing after it
  becomes set;
- the inverse set-to-clear transition;
- A1000 BBUSY becoming visible between decode and comparison;
- static BFD-clear and BFD-set `WAIT` and `SKIP` cases;
- byte-stable snapshot restore while a `SKIP` comparison is pending;
  and
- deterministic continuation after restore.

## Drift triggers

Reject these patterns:

- evaluating `SKIP` directly when its instruction pair is decoded;
- storing a pending condition without preserving whether it is
  `WAIT` or `SKIP`;
- feeding internal rather than externally visible blitter busy to BFD;
- letting `SKIP` enter the persistent waiting state; or
- reconstructing the pending instruction kind during snapshot restore.

## Related Documents

- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
