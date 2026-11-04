# CPC adopts corrected Z80 WAIT execution

**Date:** 2026-11-04
**Status:** ACTIVE — user approved CPC v6 and rejection of v4/v5
**Scope:** CPC464, CPC664 and CPC6128; emu198x/emu198x#2284

The CPC common wiring and runtime consume published Z80 0.9.0. This explicitly
supersedes the CPC 0.8 adoption boundary in
[`amstrad-wait-contention.md`](amstrad-wait-contention.md). It preserves the
physical WAIT mechanism and phase 2 release described in
[`cpu-stalling-is-wait-or-clock-not-both.md`](cpu-stalling-is-wait-or-clock-not-both.md).
No CPU clocks, machine scheduling, chip fields or hardware phases are added.

CPU 0.9 samples memory WAIT at T2 falling and I/O WAIT after its automatic TW,
then inserts complete TW pairs. The first physical divergence in the existing
synthetic firmware is the CRTC index-port write at BC00: the old CPU releases
IORQ/WR at master half-cycle 81, the corrected CPU at 87. WAIT is already
asserted again when the corrected I/O sampling edge arrives. Ignoring it to
retain the old fixture would preserve the CPU bug.

All three original v5 specimens reproduce with CPU 0.8. Corrected fresh
execution reaches the same time and monitor scan but a different CPU position.
The next three fields have identical pixels, audio sample bits and durations;
the final complete saved state differs. Identical field output does not make
the saved executions equivalent.

The runtime writes version 6 and rejects v4/v5 from their version alone, before
decoding the positional payload or mutating the live runtime. No approximate
reader is supplied. Every original v4/v5 specimen remains unchanged; separate
v6 specimens pin complete fresh and resumed execution on all three owners.

Qualification includes real 464 firmware, 664/6128 AMSDOS catalogue and program
execution, and all five existing SHAKER tests. Test injection must use
`at_execution_boundary`: the older `instruction_complete` flag can remain true
during the following fetch. Changing PC there corrupts the probe's entry.

This adoption does not settle the existing #971 mixed-stream/acknowledge
discrepancy: the observed JP/NOP pair remains 11/5 T-states, and IM1 entry
remains 19. The firmware guide gives whole-microsecond costs; Arnold and
Caprice32 tabulate totals, while MAME pads memory accesses. Those precedents
do not independently qualify a new physical release phase. Keep the existing
phase and require a discriminating hardware observation before changing it.

Primary provenance remains in the umbrella CPC firmware guide and service
manuals (`reference/by-system/amstrad-cpc/`). Longshot's CRTC Compendium 1.11
pp. 31, 282, 286 and 290 provides the I/O timing diagrams, instruction costs
and WAIT discussion; it is available from <https://shaker.logonsystem.eu/>.
Independent implementations are held under `emulators/amstrad-cpc/` and
`emulators/multi-system/mame/` in the umbrella.

Reconsult this record before accepting legacy CPC saves, adjusting the WAIT
phase to match a fixture, or treating a per-instruction table as a pin model.
