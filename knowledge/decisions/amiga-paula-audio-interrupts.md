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

CPU-fed playback uses the same delayed IRQ stage and the existing Idle/Playing
states. Gate idle DAT startup on clear INTREQ, present the first high byte
immediately, and leave active DAT writes in the holding latch. Retain the
output buffer while the low byte plays.

Sample the manual stop condition one CCK before low-byte expiry, or on
low-byte entry for period 1. Retain `None`, `Some(false)` and `Some(true)`
distinctly; a later INTREQ write cannot change a sampled decision. Request
the attachment-selected word IRQ even when the decision stops playback.
Volume attachment transfers at startup/high-byte entry; period attachment
transfers at low-byte entry. Channel 3 has no attachment target.

The user approved this extension and snapshot version 59, rejecting 58.
The saved decision is exposed in all four channel query leaves. Live
OCS/ECS/AGA restore checks both decisions at whole and half CCKs, including
periods 1 and 65,536.

The [manual-playback observations](../../../../reference/by-system/commodore-amiga/2026-paula-manual-playback-observations.md)
record why we follow WinUAE's early sample despite vAmiga/Minimig checking at
the boundary. WinUAE's maintainer explicitly credits a test correction; we
have not recovered that original hardware test package. The regression
executes 4,312 boundary observations and 576 attachment state/IRQ/target-register
observations against compiled WinUAE methods, plus the 480-row vAmiga
startup/holding schedule. Raw DAC behaviour of attached (muted) channels,
live attachment switching, DMA-to-manual transitions, PWM and analogue
response remain outside this correction.
