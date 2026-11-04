# Decision: the Amstrad 40077 drives WAIT

**Date:** 2026-11-03
**Status:** ACTIVE — approved by the user for #2265
**Scope:** Spectrum +2A, +2B and +3 using the 40077 gate array

The 40077 keeps the CPU clock running and drives the Z80 WAIT input. This
explicitly overrides RULES.md clock rules 2–3 where they imply that this chip
suppresses CPU edges. The master oscillator still schedules every component;
there are no extra clocks or catch-up execution. Ferranti clock gating remains
unchanged.

The Amstrad +3 manual, printed pp. 199 and 273, distinguishes its refresh
behaviour and explicitly describes wait-state contention. Its service circuit
diagrams wire IC2 NWAIT pin 24 separately from CLK. Primary provenance is held
under `reference/by-system/sinclair-zx-spectrum/` in the umbrella. SpecIde's
`generateVideoDataGa` provides independent implementation precedent for the
physical signal and one-in-eight RAM slot phase. FUSE and MAME boot the original
Cybernoid disk; transparent MAME I/O observations capture the complete transfer.

Suppressing clocks on refresh MREQ incorrectly slows uncontended code when I
addresses contended RAM. The paced FDC exposed that existing fault. Do not hide
it by removing disk-byte pacing, enlarging overrun deadlines or disabling
contention.

The CPU samples WAIT at its documented falling edges and inserts complete
TW pairs. Qualification must include unsampled pulses, refresh immunity,
stable strobes, no early retirement and exact paused continuation. Reusing
existing phases does not change the CPU's serialized layout. Any additional
CPU schema change needs its own approval.

The gate array saves its live WAIT output. These three machine models write
snapshot v20 and reject v19 before payload decoding. Other Spectrum profiles
keep their own existing boundaries. Original v19 specimens remain rejection
evidence; expected catalogue hashes remain unchanged until each difference
has been explained and qualified.

Reconsult this decision before gating an Amstrad CPU clock, fitting a delay
mask to a loader, changing WAIT sampling or weakening a disk overrun check.

## CPU adoption boundary

Approved 2026-11-04: the Spectrum family explicitly consumes published Z80
0.9.0 with the corrected WAIT sampling; the 40077 is published as 0.3.0.
Other families retain their existing 0.8 requirement. CPC's frozen v5 execution
changes under the correction, so its upgrade requires independent timing and
save-policy qualification in #2284. The existing approval does not authorize a
CPC save-layout or legacy-reader change. Dependency-graph checks must prove the
intended separation rather than relying on manifest text alone.
