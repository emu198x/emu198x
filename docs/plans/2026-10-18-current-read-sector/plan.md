# Track the sector currently being read

Lane: Spectrum launch-hardening. Approved behaviour: current-sector/error
tracking first, interrupted reads and native save/restore, then two real-ROM
regressions and an incorrect expected-execution control. Exact service windows
remain separate under #1831. Keep the Fuse Scan candidate unsubmitted.

1. Reproduce current-R leakage and Read Track's loss of already-observed errors
   in `crates/nec-upd765a/tests/read_progress.rs`; retain red logs. Compare stock
   Fuse/MAME controller paths and held NEC read/error descriptions. Do not infer
   disputed physical CHRN or timing from another chip revision.
2. Refactor selection into one-sector-at-a-time read preparation in
   `crates/nec-upd765a/src/lib.rs`. Preserve the existing byte clock/start policy,
   size limits, mark/SK rules, marginal-byte recipe and clean/error end results.
   Keep command arguments distinct from current transfer position. Apply mark
   status when encountered and data CRC/status when the field completes; do not
   touch unread later sectors or their reread counters. Read Track keeps its
   independently observed command-CHRN result convention; physical ordinal is
   internal progress, not an invented result-header rule.
3. Approved 2026-10-18: serialized `ReadProgress` holds the current field's pending status
   and Read Track ordinal. User approved
   Spectrum snapshot v5→v6, rejecting v5; CPC remains unchanged. No new dependencies or host interfaces.
4. Add public-register tests before/within/between sectors, future-error leakage,
   mark/CRC combinations and result draining. Add runtime snapshot tests at
   pending first request, RQM high/low, sector transitions and errors/results in
   `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`. Assert source media
   immutability and interrupted/uninterrupted continuation agreement.
5. Run controller/machine/runtime tests, Clippy, fmt, normal locked headless
   build. Reuse umbrella ROM regression for two cold boots and a wrong-signature
   rejection; label captured assembly/mastering separately. Retain independent
   source-controller observations with exact source/tool/artifact identities.
6. Push an owning-repo PR and inspect CI before merging. Follow normal release
   workflow. Update #1831/#84 and the existing umbrella progress view without
   closing broader conformance work or inheriting family coverage. Verify remote
   retention, clean task worktree/builds/processes and preserve safe evidence.

Design note: streamed selection changes when errors become observable, not the
number of emulated clocks per byte. Inter-sector/index/head timing and exact
sub-byte deadlines remain explicitly unqualified. Keep H/N/MT/DTL semantics
outside this bounded change rather than silently expanding the command model.
