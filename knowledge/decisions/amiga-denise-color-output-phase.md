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

The neutral COLOR00 program independently places the OCS edges at marker-relative
hires samples 262, 278, 294 and 310 in both vAmiga and FS-UAE. Emu198x's
common pre-output queue placed them two samples later. See the
[primary observation record](../../../../reference/by-system/commodore-amiga/2026-neutral-video-output-phase-observations.md).
This resolves the implementation-family question for the tested OCS profile;
it does not establish physical hardware timing.

CPU and debugger writes are a different scheduling case. They are dispatched
after the current output work in the machine tick and must be available to the
next tick without crossing the Copper's pre-output stage.

## The decision

The machine driver distinguishes a Copper MOVE from an ordinary custom-register
write.

An ECS or AGA Copper `COLORxx` MOVE dispatched before output enters the early
Denise-side RGA stage. The current lores output tick retains the preceding colour and the
write becomes chip-visible after that tick. On AGA, the resulting Lisa palette
write then crosses the additional one-hires-sample stage defined separately.

On OCS, the pre-output write updates the chip before the current output tick.
It bypasses the ECS/AGA queue. This is a concrete-chip dispatch policy; neither
the beam coordinate nor the framebuffer crop changes.

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

Both independent implementation families agree on the neutral program's
marker-relative colour phase. The A500 Test Kit gradients and EBU bars now
require exact equality with the unchanged producer images. Physical capture
could still overturn the model. No reference image, absolute crop or channel
normalisation was changed to obtain agreement.

## Verification

Focused tests establish that:

- OCS pre-output colour is present on the dispatch output tick;
- ECS/AGA Copper colour remains pending through the current output tick;
- a post-output CPU or debugger write is ready for the next tick;
- the board pending stage survives serialization;
- AGA crosses the common stage and then Lisa's one-hires-sample stage;
- palette-write diagnostics remain populated on both dispatch paths; and
- all ten programmable-HBLANK write-timing observations retain their exact
  registered UAE-family signatures.

Both profile Test Kit contracts require exact agreement for all six cases,
including both alternating-checkerboard phases.

## Related Documents

- [AGA Lisa colour-output delay](amiga-lisa-color-output-delay.md)
- [Lisa bitplane and display-window output phase](amiga-lisa-bitplane-diw-output-phase.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)
- [Amiga programmable-HBLANK conformance](../processes/amiga-programmable-hblank-conformance.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)

## Lisa counter-origin correction (2026-10-06)

The approved counter-origin investigation supersedes the AGA early-queue
policy above. A separate COLOR00 guest emits four adjacent Copper MOVEs. The
reference changes at counters 292.5, 300.5, 308.5 and 316.5; the native queue
added one lores tick to every edge. Lisa's early handler now enters its
existing two-sample palette stage directly. CPU/post-output handling and the
one-hires delay remain unchanged. Snapshot 53 retains the old pending field.

The evidence does not independently recalibrate ECS Copper colour timing;
that policy remains unchanged. The original raw-reference comparisons retain
their historical results but their uncorrected origin cannot prove absolute
phase. The counter-origin observations in the shared reference library record
the measured origin and control identities.

## ECS counter-origin correction (2026-10-07)

The independently traced A500+ colour control also disproves the ECS early
queue. All four edges are one lores tick late, with 256 differing samples per
field in all three captured fields. The shared reference observation record
`2026-ecs-colour-blanking-observations.md` identifies the guest and counters.
The user approved this investigation and bounded correction on 2026-10-07.

ECS now handles the pre-output write in its existing concrete-chip hook,
matching the reference's immediate early-RGA palette update. OCS is unchanged;
Lisa retains its independently verified one-hires palette delay. This
supersedes the ECS queue policy and associated absolute-phase claims above.
The saved board queue remains in snapshot 53 for layout compatibility; no new
write enters it on the installed OCS/ECS/AGA variants.
