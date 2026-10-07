# Paula period counter

**Date:** 2026-10-07
**Status:** BINDING

Keep AUDxPER as a 16-bit register. Reload the existing digital counter with
the written value, interpreting zero as 65,536 CCKs. The recommended DMA
sampling period is not a counter floor. Period writes change the latch and
do not restart an in-flight sample interval.

The [primary observations](../../../../reference/by-system/commodore-amiga/2026-paula-period-observations.md)
and `test-data/commodore/amiga/paula-audio/period-probe/` retain executable
vAmiga reload/write evidence and the native failures. This is component-source
agreement, not physical-hardware calibration or proof of DMA-starvation and
modulation behaviour.

The user approved widening the existing saved counter and public derived
diagnostic period fields to `u32`, with Amiga snapshots advancing to version
57. This gives the full countdown an explicit, inspectable representation.
The programmed register, clock source and DMA pipeline remain unchanged.

Do not restore a low-period clamp to hide missing DMA requests or buffer
behaviour. Investigate those at their own signal stages. Likewise, do not
change the analogue filter to conceal a digital sample-timing discrepancy.
