# Decision: Separate Copper colour writes from post-output writes

**Date:** 2026-08-08
**Status:** BINDING

## The question

When does a Copper `COLORxx` write become visible relative to the output tick
in which the Copper MOVE is dispatched?

## Evidence

The registered FS-UAE programmable-HBLANK write-timing package fixes the
producer and Emu198x framebuffers to one beam-absolute horizontal mapping. In
all ten ECS and AGA cases, the visible Copper colour marker begins one lores
output tick after the MOVE position. AGA then retains the preceding palette
value for Lisa's separate one-hires-sample colour stage.

The same mapping exposed an older Test Kit normalisation error. A crop derived
from bitplane content was two host-HIRES samples early and made a correctly
timed colour edge appear late. Correcting the bitplane phase and using the
beam-absolute crop makes the A1200 EBU bars exact without changing any
producer pixel.

The registered OCS family disagrees. vAmiga applies a Copper colour change at
its current Agnus pixel position, and the A500 Test Kit gradients and EBU bars
retain the corresponding two-sample difference. Its checkerboards, dots and
crosshatch remain exact. This is an implementation-family disagreement, not
evidence that either family represents physical OCS hardware.

CPU and debugger writes are a different scheduling case. They are dispatched
after the current output work in the machine tick and must be available to the
next tick without crossing the Copper's pre-output stage.

## The decision

The machine driver distinguishes a Copper MOVE from an ordinary custom-register
write.

A Copper `COLORxx` MOVE dispatched before output enters the early Denise-side
RGA stage. The current lores output tick retains the preceding colour and the
write becomes chip-visible after that tick. On AGA, the resulting Lisa palette
write then crosses the additional one-hires-sample stage defined separately.

A CPU or debugger colour write dispatched after output updates the concrete
chip immediately. It does not enter the pre-output queue. Lisa still applies
its own colour-output delay because that delay belongs to the concrete AGA
pixel path rather than to Copper scheduling.

Non-colour custom-register writes retain their existing concrete-chip
propagation rules. The Copper-specific dispatcher must not duplicate the
complete machine register map; it specializes only the phase-sensitive
`COLORxx` range and delegates everything else to the ordinary dispatcher.

## Persistence and inspection

A queued pre-output colour write can affect the next output tick after a
save-state boundary. The board-level pending queue is therefore serialized and
reported through the Denise pipeline diagnostic snapshot. Lisa's subsequent
one-sample pending value remains separate state.

Palette-write diagnostics record both Copper and ordinary writes with the same
CCK, CPU context and selector state. Separating dispatch paths must not make
Copper writes disappear from inspection.

## OCS evidence boundary

Emu198x retains the common early stage while the OCS conclusion is unresolved.
The A500 gate treats gradients and EBU bars as exact registered disagreement
signatures and continues to require exact reference agreement for the four
non-raced cases. A passing mixed contract therefore means that the known
disagreement is unchanged; it does not mean that Emu198x matches vAmiga or
physical OCS output for those colour transitions.

Physical capture or another independent implementation family is required to
resolve the OCS phase. Rebaselining the vAmiga images, moving their absolute
crop, or dropping the two cases would destroy the disagreement evidence.

## Verification

Focused tests establish that:

- a Copper colour write remains pending through the current output tick;
- a post-output CPU or debugger write is ready for the next tick;
- the board pending stage survives serialization;
- AGA crosses the common stage and then Lisa's one-hires-sample stage;
- palette-write diagnostics remain populated on both dispatch paths; and
- all ten programmable-HBLANK write-timing observations retain their exact
  registered UAE-family signatures.

The profile Test Kit contracts separately pin exact matches and known
comparator disagreements.

## Related Documents

- [AGA Lisa colour-output delay](amiga-lisa-color-output-delay.md)
- [Lisa bitplane and display-window output phase](amiga-lisa-bitplane-diw-output-phase.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga programmable-HBLANK conformance](../processes/amiga-programmable-hblank-conformance.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
