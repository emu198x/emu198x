# ZEX validation and checkpoint provenance

Full `run_zexdoc` and `run_zexall` runs start from cold boot. They execute the
selected COM binary and require all 67 checkpoints plus the completion line
in that run. A saved prefix cannot stand in for tests on the current CPU.
The full-result assertion also rejects a result marked as resumed.

Checkpoint snapshots may still be written during a full run. They are used
only by explicit targeted checkpoint runs, which select the highest available
checkpoint before the requested target. Those runs are debugging aids, not a
claim that the inherited prefix was revalidated on the current build.

## Cache identity

Snapshot format version 2 stores the original COM bytes and compares them
exactly with the selected input before a targeted resume. Identity is independent
of mutable emulated RAM, so self-modifying code does not invalidate its own
cache. Suite, schema version and checkpoint-count checks remain in place.
The binary and snapshots remain local inputs; they are not added to Git.

Version 1 snapshots lack input identity and are rejected for targeted resume.
To regenerate checkpoints, use a fresh `EMU198X_ZEX_SNAPSHOT_DIR` and run the
suite from cold boot. Full runs ignore existing caches and replace checkpoints
as they execute them. Do not treat a cache as a trustworthy interchange format:
this is input matching, not authentication of its contents.

## Verification

Synthetic regression coverage includes a cached machine with 67 inherited
OKs poised to print `Tests complete`, while the selected COM is only HALT.
The full run must execute HALT, report no checkpoints and no completion,
and never resume. The preceding harness resumed that cached prefix instead.

Other tests verify exact-binary matching, rejection of a one-byte-different
COM, preservation of self-modified RAM when the original input matches,
legacy-cache rejection and selection of the highest available checkpoint.

These are harness tests, not new full ZEX accuracy results. Targeted caches
are not bound to a CPU source revision: regenerate them after CPU changes
when investigating repeatable behaviour. Full gates avoid that ambiguity by
executing the entire selected binary anew.
