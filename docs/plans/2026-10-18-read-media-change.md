# Abort a read when its selected disk changes

Lane: Spectrum launch-hardening. This fixes a host media-change boundary;
it does not establish physical READY timing or interrupted-result conformance.

1. Preserve the public-register reproduction against c5586b620 and a62d05994:
   replacement can combine cached R1 from the old image with R2 from the new
   image. Read Data, Read Deleted Data and Read Track are affected. Ejection
   also leaves cached data active; changing an unrelated drive must not abort.
2. Add `crates/nec-upd765a/tests/read_media_change.rs` covering replacement and
   ejection before the first request, while requests are high/low, and between
   fields. Verify immediate result, no further payload, retained encountered
   status/current R, drained results and a fresh read of replacement media.
3. Extend the existing selected-drive media-change abort helper in
   `crates/nec-upd765a/src/lib.rs` to all three read commands. Use existing
   NR/abnormal policy and read_result cleanup; keep write/format/Scan behavior.
   No new dependency, API, serialized field or version change.
4. Retain failing and passing logs; run all controller tests, Clippy and fmt.
   File the narrow owning issue, commit/push, and hand the commit to the parent
   session for PR1833 integration. Do not create another PR or alter its files.
5. Preserve safe evidence, verify remote retention and remove isolated
   checkouts/build caches after parent integration. No upstream posting.

Results, exact source comparisons and bounded reference interpretation are in
[the report](2026-10-18-read-media-change/README.md). Owning issue:
[1836](https://github.com/emu198x/emu198x/issues/1836).
