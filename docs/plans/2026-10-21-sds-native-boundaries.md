# Prove SDS native continuation at command/result boundaries

Lane: Spectrum launch-hardening. Owning issue:1910. The agreed change is test
coverage of the existing nativev9 contract, with no new API/dependency/schema.

1. Reuse `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs` synthetic media,
   direct FDC command and snapshot helpers. Add an explicit four-boundary matrix:
   idle, opcode-only/parameter-pending, result-pending, result-drained. Exercise
   all32 input vectors and all8 logical unit/head selections; physical A/B receive
   opposite levels. Assert exact one-byte/MSR framing and alias selection.
2. Change signals before the parameter and after latching. Compare uninterrupted
   versus restored results, byte-identical immediate round trips, full final
   state, source/live media immutability and subsequent SDS. Continue/drain
   retained READY events without granting polling/mechanical timing credit.
3. Emit one bounded JSON receipt through the existing test evidence mechanism,
   without saving native states or firmware. Require exact1024-case coverage and
   nonzero assertions. Deliberately substitute an idle checkpoint for the
   parameter-pending state and/or a wrong expectation; retain the intended red
   result, restore the unmodified implementation and show green.
4. Run the matrix twice, existing controller/+3/runtime suites, nativev8 rejection,
   fmt and scoped all-target Clippy using a task-owned target directory and
   bounded host execution. Inspect failures before any production change; stop
   for approval if a real API/schema change proves necessary.
5. Push a test-scoped Emu PR, inspect actual applicable CI and closing references,
   merge when verified. Keep component source execution separate from existing
   released normal-ROM guests. Do not grant a new SDS operation verdict here.
6. Retain portable machine-readable evidence and a readable visual status in the
   umbrella, update1910/1876 and programme tracking, and identify the remaining
   operation-auditor gate. Verify evidence/remote retention, then remove the new
   Emu worktree and its disposable target directory. Preserve canonical work and
   the retained umbrella/sample reproduction checkouts.
