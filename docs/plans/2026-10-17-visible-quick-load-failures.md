# Explain failed quick-loads with native feedback (#1816)

Lane: Spectrum launch-hardening. The user approved the existing native warning
dialog pattern rather than a title-bar notice. The dialog pauses emulation
until OK; the machine remains usable afterwards. No dependency or public API
change, custom overlay, save-format change or unrelated UI rewrite.

Root cause: `App::quick_load` logs missing-folder/read/restore failures and
returns `Ok(())` without a visible result. The real native audit proved dispatch,
missing-file refusal, successful save/restore and corrupt-file recovery; the
user mistook the silent missing-file path for an inactive shortcut (#1816).

1. In `crates/emu198x-ui/src/quick_load.rs`, isolate the existing bounded host
   file-read/restore operation and return actionable failure text, retaining
   stderr diagnostics. Test real absent paths, unreadable input, invalid bytes
   and subsequent valid loading without opening a native window. Keep restore
   semantics owned by the runtime; do not change core snapshot logic.
2. Update `crates/emu198x-ui/src/lib.rs` to use that helper and the existing
   `rfd::MessageDialog` warning pattern on failure. Release held host input before
   the dialog and re-pace after dismissal. Keep the successful refresh path and
   menu/shortcut command routing intact. No success dialog.
3. Run focused UI tests, formatting, Clippy and a Spectrum build. Use debug-info
   and incremental settings that bound this task's disposable target cache;
   available disk space starts at 17 GiB. Do not remove another task's caches.
4. Check the actual native missing/invalid dialogs and recovery on macOS if CUA
   is available, otherwise prepare an isolated source-build handoff for a human
   check. Label tests/builds separately from native visual evidence. Preserve
   normal save slots via existing `EMU198X_STATE_DIR`.
5. Commit/push a small PR with verified scope and limits. Do not claim the new
   behaviour ships in an old published binary. Inspect task worktree, target,
   processes and handoff before cleanup; verify remote retention first.

Current reuse: the shared tape-export handler already uses native `rfd`
message dialogs. The three named external donor archive paths are absent;
no donor code is copied. The native-UI decision binds winit/muda/rfd, so the
fix introduces no rendering framework or replacement native UI.

## Verification

- UI library: 30 tests pass, including four quick-load cases using actual
  missing, directory and truncated fixture files. These tests exercise the
  host helper, not a native window or the real snapshot decoder.
- Removing the missing-slot recovery guidance makes its focused test fail at
  `assertion failed: explanation.contains("Save a state first")`; restoring
  the guidance returns the complete suite to green.
- Spectrum runtime: all five existing `decode_rejects` tests pass.
- UI formatting, Clippy with warnings denied, Spectrum candidate build and
  whitespace checks pass. No dependency or lockfile change.
- Native control still reports `Sky Computer Use native pipe startup failed`.
  Prepare a local isolated-state handoff for missing/invalid warnings, valid
  restore and post-dismissal input. Native confirmation remains pending;
  this is a source-built candidate, not evidence for a published release.
