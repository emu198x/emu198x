# Explain failed native media opens (#1843)

Lane: Spectrum launch-hardening. The user approved the existing quick-load
native warning approach for File → Open and approved merging audit PR #188.
No dependency, public API, media decoder or save-format changes.

Root cause: the real macOS audit of published 0.36.0 rejected two malformed
TAP selections and later loaded valid media, but `App::open_media` only writes
read/runtime failures to stderr. The user gets no visible explanation. The
runtime's safe rejection and subsequent execution were separately checked.

1. In `crates/emu198x-ui/src/media_open.rs`, isolate the selected-path read and
   runtime-load operation. Keep `read_media_asset` archive/format handling and
   diagnostics. Keep cancellation in the existing picker branch; return actionable
   failure text from the helper. Use actual temporary files to check read failure,
   runtime rejection and a later successful insertion without mutating input.
2. In `crates/emu198x-ui/src/lib.rs`, call this helper after the existing native
   picker. Show an Open Media warning only on failure, release held input and
   reset pacing after dismissal, as quick-load already does. Preserve valid
   refresh behaviour and quiet cancellation. Do not refactor other dialogs.
3. Run focused UI tests, a deliberate failing control, format/Clippy and a
   Spectrum build with debug info/incremental disabled in a disposable target.
4. Prepare an isolated source-build handoff reusing the retained audit tapes
   locally. Attempt CUA, otherwise ask for the native cancellation/warning/
   recovery checklist. Keep native evidence distinct from unit tests.
5. Commit/push a draft PR pending native verification. Verify remote retention,
   inspect temporary files/processes, remove the finished worktree/build cache;
   keep only the handoff until results are retained and its process is closed.

Reuse: current quick-load and tape-export warnings use `rfd::MessageDialog`.
The three named donor archives are absent. The locked winit/muda/rfd design
and the approved native warning pattern remain binding; no donor code needed.

## Verification

- All 33 UI library tests pass. Three new cases use real missing files,
  damaged archives and selected media; runtime callbacks test propagation,
  routing, read-only input and subsequent success, not the actual TAP decoder.
- A temporary mutation that swallowed runtime errors made the rejection test
  fail with `runtime rejection must reach the user: ()`. Restoring the code
  returns the complete suite to green.
- UI formatting, Clippy with warnings denied, whitespace and the Spectrum
  source-candidate build pass. No dependency or lockfile changes.
- CUA still reports `Sky Computer Use native pipe startup failed`. Actual
  native warning, quiet cancellation and post-dismissal recovery remain pending
  the isolated local candidate checklist; unit tests do not establish them.
