# Paula DMA interrupt stages

**Date:** 2026-10-07
**Status:** BINDING

Retain the DMA loop condition independently of the request awaiting INTREQ
visibility. A wrapped word sets the loop condition; the attachment-selected
output transition consumes it. Both startup and selected loop requests enter
one CCK of delivery delay. Stopping DMA discards an unissued loop condition,
but cannot cancel an already-issued request.

Deliver pending requests at the existing shared CCK boundary before retained
DMA retirement. Run sample output at its existing phase. The component combined
tick wraps the same begin/end phases. Add no second clock or catch-up ticks.

The user approved these existing-stage extensions and snapshot version 58,
which rejects version 57. Both pending flags are saved and queryable. Live
OCS/ECS/AGA restore must preserve the held condition, selected edge and delayed
request, including snapshots halfway through a CCK.

The [primary observations](../../../../reference/by-system/commodore-amiga/2026-paula-interrupt-observations.md)
separate the original manual from executable vAmiga component evidence and UAE
source corroboration. The DMA regression checks all 4,896 reference rows;
board tests verify actual retained-DMA startup delivery.

Manual CPU-fed playback remains a separate fault. Its runnable research probe
retains 396 mismatches in 480 observations; do not claim manual accuracy from
the DMA regression. Resolve the UAE/vAmiga acknowledgement sampling disagreement
before choosing the manual stop edge.
