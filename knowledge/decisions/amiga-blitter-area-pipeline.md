# Decision: Amiga area-blitter channel and holding stages

**Date:** 2026-10-05
**Status:** ACTIVE

Area blits advance through saved channel/fill phases. The user approved this
extension and the version-49 snapshot break on 2026-10-05.

The root cause and primary/reference evidence are recorded in
[the shared observations](../../../../reference/by-system/commodore-amiga/2026-area-blitter-stage-observations.md).
The previous engine offered only enabled channels. D-only writes consequently
ran at one CCK per word instead of the reference's two, and the engine could
not retain overlapping source/previous-output holding stages.

The existing CCK scheduler admits one phase per granted free DMA cell.
Source-defined idle and locked first-D cells yield the physical chip bus to
the CPU, but cannot advance through an occupied DMA cell. No elapsed delay,
second scheduler, clock counter, synchronous machine access or CPU callback
replaces these stages.

A and B shifted holding registers retain their values separately from newly
fetched data. The result phase consumes the preceding held source word;
ordinary D transfers write that retained result while the next word moves
through its source stages. The first D phase is locked. Source pointers
apply modulo at their own row ends; D and fill-result row counters follow
the delayed output. Masks apply to the source column before its A hold.

The last main phase defines F. D-disabled blits retire their final result
and BZERO there. D-enabled blits compute their final result at F+1 and request
the final D at F+2, retaining the separate revision-dependent interrupt and
busy observers in the
[completion decision](amiga-blitter-completion-pipeline.md). The preceding
word may still write on F; that does not make the final buffered D ready.

Save states retain phase, pipeline priming, shifted holds, pending result and
separate destination/result row counters. Version 49 rejects version 48
before decoding its incompatible payload. Do not reconstruct these fields
from register values on restore.

The source-table adapter checks all 32 main DMA schedules. It does not prove
physical silicon timing or every data path. Existing all-disabled data/BZERO
semantics remain unchanged, beyond correcting its two-cell schedule.
[Mid-blit register propagation](amiga-mid-blit-register-writes.md) remains
bounded by the prior decision; this change does not make every captured
runtime parameter respond to a live register write.
