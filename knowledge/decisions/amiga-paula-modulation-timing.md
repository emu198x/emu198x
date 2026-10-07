# Paula modulation and DMA transition timing

**Date:** 2026-10-07
**Status:** BINDING

Use the existing byte phase and AUDxDAT holding latch for attachment. Startup
and high-byte entry deliver volume; low-byte entry delivers period. A source
channel's output-event word is not the attachment input. Channel 3 has no target.

Request ordinary/volume DMA at high-byte entry and period DMA at low-byte
entry. Period-only startup waits for its first low-byte transition before
requesting the next word. Retain one pending request, repeat the DMA output
buffer during underflow, and load a waiting word on high-byte entry so a word
arriving between the byte edges is not delayed by a full word.

The [primary observations](../../../../reference/by-system/commodore-amiga/2026-paula-modulation-observations.md)
cite the original HRM and distinguish executable vAmiga component evidence
from UAE source corroboration and native output checks. The retained corpus is
`test-data/commodore/amiga/paula-audio/modulation-probe/`.

This correction reuses the saved fields, clock and stages; snapshot version 57
is unchanged. Both component DMA entry paths must agree. Do not restore a
request backlog or move the holding/output transfer to the preceding low byte.

The result does not establish manual startup/IRQ accuracy, live attachment
switching, target PWM timing or physical analogue response.
