# Read ID represented CRC errors

Lane: Spectrum launch-hardening. Fix the observed Read ID false success before adding further qualification.

1. Reproduce ID-only DE metadata returning success in `crates/nec-upd765a/src/lib.rs`. Record the failing assertion.
2. Consult NEC December 1978 pp14,17–18 and unchanged Fuse `read_id`/`start_read_id`, plus independently audited raw MAME controls from #1860.
3. In the ReadId arm only, project represented ID CRC (ST1 DE set, ST2 DD clear) to abnormal termination, DE and ND. Preserve CHRN, rotational progression, source disk and recovery. Ignore data-field status; combined ID/data damage is not distinguishable in sector metadata.
4. Test clean/ID/data status, all result bytes, head/drive echo, progression and source immutability. Show the focused test red on unchanged production source, then green after the fix. Run all controller tests, Clippy and formatting.
5. Retain bounded source evidence and reference provenance; no new API, dependency, fields or snapshot format. Qualification remains false; this is controller regression evidence. Push an owning PR for review; parent coordinates merge/release.

Exact physical timing, index search and ready-pin behaviour remain outside this sector-model fix. MAME extra MA on bad-ID Read ID is a separate disagreement; do not emulate it without hardware evidence.
