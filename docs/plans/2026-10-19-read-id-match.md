# Match all recorded ID fields before reading sector data

Lane: Spectrum launch-hardening. Scope: Read Data and Read Deleted Data ID
selection only. Preserve result CHRN, byte pacing, Read Track, MT and DTL.

1. Read current code, held NEC December1978 command definitions and the stock
   Fuse seek_id / MAME sector_matches implementations. Record exact identities
   and distinguish source inspection from executed reference observations.
2. Add public-register regressions in
   `crates/nec-upd765a/tests/read_id_match.rs`. Prove wrong H/N rejection,
   later matching duplicate selection, nonphysical recorded H requested
   explicitly, selected physical side independent of ID H, both data marks,
   full seven-byte results, recovery and source media immutability. Retain red
   logs against source1a17cf4779f397c44de03312bacefd39cfa9083a.
3. Report findings before changing production. Once reproduced, file the narrow
   owning issue. Extend only the non-ReadTrack predicate in
   `crates/nec-upd765a/src/lib.rs` to compare recorded C/H/R/N to requested ID.
   No dependency/API/schema changes. Result H remains a separate observation.
4. Run focused and full controller tests, Clippy, fmt and meaningful evidence
   identity/red-green checks. Retain safe source/log evidence under this plan.
5. Commit/push an owning branch and PR; parent runs runtime/ROM validation and
   handles release sequencing. Verify remote retention and clean task workspace,
   disposable build cache and task-owned processes before completion.
