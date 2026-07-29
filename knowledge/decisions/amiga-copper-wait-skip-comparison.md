# Decision: Copper WAIT and SKIP comparison phase

**Date:** July 2026

## The question

How does the Copper evaluate the beam and blitter-finished condition during
`WAIT` and `SKIP`?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed pages 20–21
and 30–32, defines the instruction formats, masked beam comparison and
aggregate instruction costs. It assigns two memory cycles to `MOVE` and
`SKIP`, three to `WAIT`, and applies the position-comparison rules to both
conditional instructions. Appendix A, printed page 275, states that BFD
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

The inspected vAmiga revision
`60fd1e6b69dcd77c9f44d1291bd37ec715362ab0` presents the horizontal
comparator two CCKs ahead of the physical beam and wraps its 227-CCK PAL
line at physical `$E0`. The change came from vAmiga issues 629 and 645;
issue 645 identifies its oracle as an OCS Amiga 500. This evidence therefore
establishes the PAL case rather than a universal `$E0` boundary.

The inspected WinUAE revision derives its terminal comparison from the active
line limit and parity. Its `coppercomp` aliases the terminal position to zero
when the current highest count is even. A WinUAE changelog entry independently
records that using `$E0` on normal NTSC and programmed even-length lines was
incorrect. Its beam advancement also retains the old vertical count for
comparator positions zero and one, then advances the vertical count before
comparator position two.

The Amiga Test Kit v1.21 gradients and EBU-bar references provide an
end-to-end observation of this offset. Literal physical-HP comparison moved
their Copper colour transitions eight runtime pixels late. Emu198x emits four
runtime pixels per CCK, so the observed displacement is the expected result of
a two-CCK comparator error.

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

For the horizontal part of that comparison, the effective position is:

```text
L = physical CCKs in the active line
P = L & ~1

comparator HP = 0                         when P = 0
comparator HP = (physical HP + 2) mod P  otherwise
```

`P` is the largest even number of CCKs in the active line. A 227-CCK PAL or
NTSC short line has `P=226`, so its wrap origin is physical `$E0`. A 228-CCK
NTSC long line has `P=228`, so its wrap origin is physical `$E2`. With ECS
programmable beam timing, `L` is `(HTOTAL & $01FF) + 1 + LOL`.

Agnus owns this physical-to-comparator projection. The shared Copper receives
the resulting horizontal value, clears HP bit 0, and applies the instruction
mask. The vertical position remains the physical beam's low eight bits. On an
even-length line, the final two projected positions zero and one retain the
old vertical position; the new physical line begins at projected position
two with the new vertical position.

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
- Copper slot polarity and terminal-slot exclusion under NTSC long lines or
  ECS programmable horizontal totals;
- the exact relationship between Copper wake-up, a yielded bus request
  and the following first-word fetch;
- the first Copper fetch after a completion-dependent wait becomes
  eligible; or
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
  and deterministic continuation after restore;
- direct horizontal-mask comparison of the Agnus-supplied position;
- PAL and NTSC-long two-CCK projection across their different wrap origins;
- ECS programmed odd/even line lengths, both LOL states and degenerate
  programmed totals;
- machine-boundary routing through original Agnus, Fat Agnus, ECS Agnus and
  Alice; and
- the full-mask line-255 crossing.

The explicit Amiga Test Kit v1.21 lane verifies the gradients, dots and
EBU-bar transitions against the registered vAmiga reference. With the
comparator correction, the gradients mismatch contracts from 7,822 pixels to
88 and the EBU-bars mismatch contracts from 16,070 pixels to 110. Both
remaining bounding boxes are confined to the Test Kit menu-pointer region.
The dots pattern becomes an exact match.

Those figures isolate the Copper correction before the later
[sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md)
change. They are not the current whole-lane result.

## Drift triggers

Reject these patterns:

- evaluating `SKIP` directly when its instruction pair is decoded;
- storing a pending condition without preserving whether it is
  `WAIT` or `SKIP`;
- feeding internal rather than externally visible blitter busy to BFD;
- letting `SKIP` enter the persistent waiting state;
- feeding literal physical HP to Copper instead of the installed Agnus's
  active-line projection; or
- reconstructing the pending instruction kind during snapshot restore.

## Related Documents

- [Amiga sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md)
- [Agnus blitter startup before the first channel operation](amiga-agnus-blitter-startup.md)
- [Blitter completion pipeline](amiga-blitter-completion-pipeline.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Live-machine save-state serialization](savestate-live-machine-serde.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
