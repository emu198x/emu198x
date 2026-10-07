# Paula DMA interrupt stages

**Date:** 2026-10-07
**Status:** BINDING

Retain the DMA loop condition independently of the request awaiting INTREQ
visibility. A wrapped word sets the loop condition; the attachment-selected
output transition consumes it. Both startup and selected loop requests enter
one CCK of delivery delay. DMA mode changes preserve the active byte pipeline
and its scheduled sampling phase. A DMA-off edge does not discard a held loop
condition or an issued IRQ; consume the held condition at its eligible DMA
transition. If it survives into Idle, DMA startup issues it through the same
delayed request stage.

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

A low-byte period entered in manual mode schedules its stop-condition sample
one CCK before expiry, or samples on entry for period 1. Retain `None`, `Some(false)` and `Some(true)`
distinctly; a later INTREQ write cannot change a sampled decision. Retain the
scheduled sample across DMA changes, recording continue if DMA is enabled at
the sampling event. A low-byte period entered under DMA has no early sample;
if DMA is subsequently disabled, use the live IRQ at expiry. Request
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
live attachment switching, PWM and analogue response remain outside this
correction.

The user approved the DMA/manual handover extension, the loop-condition rule
amendment and snapshot version 60 (rejecting 59). The existing low-byte stage
now saves `manual_stop_sample_pending` separately from the sampled decision.
Use the existing startup states only when enabling DMA from Idle; cancelling
a startup wait returns to Idle, while an active byte continues on the same
countdown. Do not reload pointers, buffers or periods on a mode edge in Playing.

The [handover observations](../../../../reference/by-system/commodore-amiga/2026-paula-dma-handover-observations.md)
record the inspected HRM state diagram and the executable reference conflict.
All 6,304 pre-output native observations agree with WinUAE, including sample,
IRQ, state and held loop condition. The paired low-byte histories must remain
distinguishable across save/restore. Board tests cover mode writes and delivery
of an already-admitted word to the holding latch after DMA clear. This does
not establish complete DMAL signalling, every startup/retirement race, or
post-output mode changes between component phases.

Propagate effective DMACON writes to the existing audio mode transitions
without advancing the clock. A retained Agnus transfer still updates its
committed memory pointer; with DMA off, deliver its word through ordinary DAT
handling and leave the DMA length counter unchanged. An idle channel starts
manual playback only with clear visible IRQ. The existing version-60 stages
already preserve this state. The [cancelled-startup observations](../../../../reference/by-system/commodore-amiga/2026-paula-cancelled-startup-observations.md)
record 704 agreeing observations from both references, the real retained-word
board regression and 192 OCS/ECS/AGA restore checkpoints. This follows the
existing register-effect boundary and does not claim a new physical DMACON
write latency or complete DMAL timing.


Treat the existing source output buffer as retained state, independently of
queued-word availability. At startup and every high-byte entry, volume
attachment diverts DAT to the next channel and preserves that buffer (also
on channel 3, which has no target). Without volume attachment, load the
persistent DAT latch. A low-byte period transfer consumes the holding-word
marker without changing the output buffer. Idle, DMA startup and cancelled
startup retain the buffer; only a chip reset clears it. Apply these rules in
the existing stages without changing the clock, request or interrupt paths.

The [live-attachment observations](../../../../reference/by-system/commodore-amiga/2026-paula-live-attachment-observations.md)
record the 60,928-row matrix and its failing baseline. The component regression
uses compiled WinUAE output for raw samples and independently agreeing WinUAE /
vAmiga buffer observations. vAmiga's repeated-edge sampler suppression remains
a documented difference. The board regression checks the audible interval
and deterministic restore at all 48 OCS/ECS/AGA checkpoints. The existing
version-60 fields hold this state; this correction changes no snapshot schema.
It does not establish physical ADKCON latency, full DMAL timing or analogue/PWM
response.


At the existing output phase, process channel transitions in ascending channel
order and apply each transition's modulation before evaluating the next
channel. If a source period transfer coincides with receiver expiry, the
receiver reloads the newly written period. Do not defer all attachment effects
until after all counters reload. Keep the same begin/end phases and pending
IRQ/request stages; this ordering correction adds no clock or saved field.

The [active-target observations](../../../../reference/by-system/commodore-amiga/2026-paula-active-target-observations.md)
record agreement on 88,704 channel observations for state, period, byte
deadline, buffer, volume register, request and IRQ across both references.
Raw DAC samples use WinUAE; vAmiga's sampler is scaled and suppresses repeated
edges. The regression covers simultaneous/adjacent deadlines and chains.
Volume-register agreement does not establish the timing of gain at the DAC;
volume output-latch timing remains a separate investigation.
