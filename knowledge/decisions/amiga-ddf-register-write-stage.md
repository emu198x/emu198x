# Decision: DDF writes reach comparators through a shared holding stage

**Date:** 2026-10-06. **Status:** approved and implemented.

DDFSTRT writes suppress the start comparator during their delivery cell.
DDFSTOP writes preserve the old stop comparator for that cell. Both commit
their masked values after the display request/comparator stage. A same-value
write still has these effects.

This follows the compiled registered functions and connected native probe in
the [primary observations](../../../../reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md#2026-10-06-ddf-register-writes-retain-distinct-comparator-phases).
It is software-reference evidence; CPU pin-to-register latency and silicon
measurement remain separate evidence boundaries.

The shared Agnus chip retains raw register mirrors, optional effective copies
and one typed pending write. Before a register's first bus write, its effective
value follows direct chip initialization. Afterwards, the saved comparator
copy controls the production display sequencer. A second write before
retirement flushes the previous shared entry in the reference order; two
independent delay queues would produce a different result.

Retirement occurs at the end of `generate_display_dma_request`, after that
CCK's request and comparator work. The shared driver already places Copper
service before this stage and CPU service after it. CPU-delivered writes stay
pending through the following comparator. Horizontal wrap does not erase the
entry. No new ticks or bus authority are introduced.

Diagnostics expose raw values, effective values and the pending register/value.
The older line-match/endpoint fields remain compatibility observations; the
saved display sequencer and reservation/address/service stages determine live
DMA. This decision supersedes earlier immediate-write assumptions in
`amiga-single-slot-authority.md` for DDF propagation, without changing its
single-authority requirement.

Runtime snapshot version 51 preserves these fields and rejects version 50
before payload decoding. All Amiga variants share this boundary. Candidate
validation rejects unmasked effective values, a suppressed comparator without
a pending start write, and inconsistent pending values before replacing the
running machine. Raw machine postcards remain unversioned; durable states
must use the runtime envelope.

Verification includes the unchanged 1,128-case reference corpus, real Copper
delivery and memory service, CPU motherboard delivery at both phases, pending
write replay on OCS/ECS/AGA across ordinary and wrap boundaries, rejection of
malformed candidate state and the version-50 boundary. The corpus's deliberate
immediate-write control reproduces the original 76 discrepant cases.
