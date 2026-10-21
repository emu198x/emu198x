# Format protection, interruption and native continuation

Spectrum launch-hardening, umbrella237. Reuse snapshot.rs's authored media,
blank runtime, native round-trip and export APIs. Existing Format helpers and
Fuse's four-CHRN commit precedent avoid a new harness or interface. No production
behaviour, dependency, API or snapshot schema change is proposed.

## Frozen component/native matrix

Use a three-sector authored EDSK with distinct old payloads and CRC/deleted
metadata. Format two IDs [7,8,255,3] and [4,5,9,0], command N0/filler6D/GPL42.
These unusual ID fields are raw recorded bytes, not physical drive geometry.

For FM/MFM and ND0/ND1, execute:
- Success at accepted-byte prefixes0..8, each before and after the next RQM:
  72 cases. Prefix8/high is pending result; prefix8/low is final fill/gap.
- Write protect, missed request service, and controller reset at prefixes0..7,
  each RQM low/high:192 cases. Initial protection is separate below.
- Host-protected mount with deasserted WP, and writable mount with asserted WP,
  at all eight pending/drained result positions:64 cases.

Total328 unique native checkpoint cases, twice. Before continuing, compare
restored status/IRQ and exact native bytes against the checkpoint. Continue
both directly and after restore. Verify exact status and success CHRN, IRQ
acknowledgement, source immutability, committed-prefix/untouched-tail payloads,
metadata and final state equality. Every terminal live EDSK must reparse and
reopen in a fresh blank runtime with identical sector content. Active export
must fail without mutation. Check rejected results at all eight result
positions as a nested continuation invariant (not extra top-level cases).

NEC December1978 Format printed14 defines N/D/CHRN and success R+1; status
table printed17 defines NW/OR. MAME format_track_start rejects initial WP;
Fuse write_data commits after four ID bytes. Partial-sector/tail retention,
mid-transfer protection and reset persistence are explicit existing digital
sector-model promises, not physical damaged-track or sampling claims. Aborted
CHRN is recorded and compared across restore but has no new silicon oracle.
Active READY and applicable Fault remain unresolved; no operation closure.

## Files and verification

Add snapshot/format_tests.rs and one include in snapshot.rs. Reuse existing
native_field_artifact for safe JSON records; never publish firmware-bearing
states. Run328 cases twice, record unique IDs and exact nonzero count. Deliberate
wrong expected success R and idle checkpoint substitution must each fail for
the intended reason. Restore originals before positives. Bound waits to one
million peripheral ticks and each host invocation to300 seconds. Run owning
chip/runtime suites, fmt and all-target Clippy. Commit/push, review/merge when
CI is green, update237 with precise scope. Inspect temporary worktree and caches
and verify remote retention before cleanup.
