# Preserve an encountered Read Track ID CRC error

Lane: Spectrum launch-hardening. Scope: Emu198x #1849; approved next bounded
experiment, not physical overrun timing or all FDC support.

1. Add public-register clean-ID, ID-CRC and data-CRC controls in
   `crates/nec-upd765a/tests/read_progress.rs`. Consume one byte and miss an
   interior request, with bounded ticks. Retain the failing baseline log.
2. Independently execute the unchanged Fuse controller against raw tracks with
   separately validated ID/data CRCs. Retain identities, bounded commands,
   repeat observations and media hashes. Do not require whole-result equality.
3. If evidence confirms it, preserve only the encountered ID-CRC-only DE at result assembly in
   `crates/nec-upd765a/src/lib.rs`. Defer data CRC until field completion.
   Reuse serialized pending status; no dependencies, interfaces or state layout
   change. Do not infer separate ID damage when DD is also set.
4. Extend runtime native-state tests in
   `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`: pending first request,
   consumed bytes, high/low RQM, result draining, overrun and uninterrupted
   continuation. Verify immutable source/live media and next-command recovery.
5. Run focused red/green checks, controller/runtime/+3 tests, fmt, Clippy and
   locked headless build. Retain two fresh ROM loader regressions and a wrong
   expected signature rejection, labelled captured assembly/mastering replay.
6. Retain auditable evidence and a readable report; push an owning PR, inspect
   checks/review, merge when ready and use normal release workflow. Update
   broader tracking without extending capability coverage. Verify remote
   retention and clean task-owned workspaces/caches/processes.

Root cause must be established before production edits. Exact silicon service
windows, completed data-CRC status disagreements, raw gaps and flux remain open.
