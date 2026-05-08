---
date: 2026-05-08
topic: native-ui-file-state-view-help
---

# Native UI: File / State / View / Help menus

## What We're Building

Four new submenus in the existing macOS NSMenu bar (currently
App + Machine), wired through the same `AppCommand` channel that
drives Machine:

- **File** — Open Snapshot... / Open Tape... / Open Disk... (each
  pops an `rfd` dialog with the matching extension filter; Disk
  is disabled unless the current variant supports a disk slot).
- **State** — Save Snapshot... / Load Snapshot... (rfd save/open
  dialogs; Load is the same action as File > Open Snapshot, lifted
  to State for discoverability).
- **View** — Window scale (1× / 2× / 3× / 4× radio items) and
  Video filter (None / CRT radio items). Replaces the CLI `--scale`
  / `--video` flags as the runtime way to change either.
- **Help** — View on GitHub / Documentation. Two URL items so
  macOS users find what they expect under Help; About already
  lives in the App submenu.

Closes Spectrum SOLID criterion 7 ("Native UI") for the October
launch — the criterion's explicit acceptance bar is "runtime file
picker, snapshot save/load, runtime window-scale selector".

## Why This Approach

**Why three explicit File items, not one combined "Open..."** — a
combined Open hides intent: if a user has a tape loaded and clicks
Open on a `.sna`, the binary has to guess whether they meant "load
this snapshot into the running tape session" or "replace everything
with this snapshot's machine state". Three items make the intent
unambiguous and give natural disabled-state semantics for the disk
slot on non-disk variants.

**Why autoload tapes by default** — clicking File > Open Tape in
any GUI Spectrum emulator universally means "I want to play this",
not "I want to inspect the editor state with this tape inserted".
Users who want the latter use the existing F10 keyboard shortcut to
stop the tape mid-load, or use `--script` / `--mcp` mode. A
file-dialog accessory checkbox to disable autoload would need
direct Cocoa bindings (rfd doesn't expose one); not worth the side
quest for the SOLID timeline.

**Why State + File both have snapshot actions** — File organises
around "load media of any kind"; State organises around "save/load
the current emulator state". Conventional split that matches user
mental model. The Load Snapshot in State and the Open Snapshot in
File dispatch identical AppCommand variants.

**Why View ships a video-filter radio alongside scale** — `View`
is the natural home for both. Scale is the SOLID criterion's
explicit ask; filter ships in the same commit because the muda
radio-group code is identical and shipping both at once costs no
extra.

**Why no Edit menu / no paste-as-keystrokes** — paste-as-keystrokes
hits the K/L/E mode hazard on 16K/48K BASIC: a pasted "PRINT"
becomes R-U-N-P-R-I-N-T... because each letter triggers different
keywords depending on editor mode. Doing it right requires either
mode-aware token injection or routing pasted-BASIC through the
LoadBasicProgram path. Out of scope for this commit; revisit when
there's a real consumer.

## Key Decisions

- **File picker library**: `rfd`. Already in workspace deps. macOS
  uses NSOpenPanel via objc2 under the hood; cross-platform behaves
  consistently.
- **Variant-aware disk slot**: File > Open Disk... is disabled
  unless the current variant's `supports_disk_slot()` returns true.
  Today only the +3 enables it; future disk-capable variants
  inherit the wiring through the existing `LiveSpectrumRuntime`
  trait.
- **Tape open behaviour**: dispatches a `LoadTape` AppCommand that
  runs the existing autoload helper after loading. F10 remains the
  escape hatch for "stop loading mid-tape".
- **Snapshot save**: rfd save dialog with `.sna`/`.z80` extension
  filter; binary writes the file via the existing
  `session.save_snapshot` helper. Default extension is `.sna`
  (the project's first-class snapshot format).
- **Snapshot restore**: rfd open dialog with `.sna`/`.z80` filter.
  Errors (file missing, parse failure, snapshot-restore disallowed
  during recording) surface as a non-fatal log message; the menu
  indicator falls back to current state.
- **Window scale**: radio items 1× / 2× / 3× / 4×. The current
  scale is checked. Switching dispatches a `SetWindowScale`
  AppCommand that resizes the wgpu surface and the OS window.
- **Video filter**: radio items None / CRT, current filter checked.
  Switching dispatches `SetVideoFilter`. Maps to the existing
  `VideoFilter::Crt` / `VideoFilter::None` from `emu198x-native-video`.
- **Help URLs**: View on GitHub points at the Emu198x repo URL.
  Documentation points at the project README on GitHub for now.
  Click handler dispatches an `OpenUrl(String)` AppCommand; the
  binary uses `open::that` (or similar) to launch the system
  browser. Add `open` to the workspace if not already present.
- **AppCommand additions**: `OpenSnapshot(PathBuf)`, `OpenTape(PathBuf)`,
  `OpenDisk(PathBuf)`, `SaveSnapshot(PathBuf)`, `LoadSnapshot(PathBuf)`
  (same dispatch as OpenSnapshot — the menu item is a separate
  surface), `SetWindowScale(u32)`, `SetVideoFilter(VideoFilter)`,
  `OpenUrl(&'static str)`.
- **Dialog-blocks-frame-loop**: while an rfd dialog is open, the
  main thread blocks. Acceptable; every native emulator behaves
  the same way. The audio sink stops naturally when no frames are
  produced.

## Open / parked items (not in this commit)

- **Edit menu / Paste-as-keystrokes** — needs mode-aware token
  injection on 16K/48K. Park for a "use the editor properly"
  follow-up.
- **Tape menu (Play / Stop / Rewind / Turbo)** — F9/F10 keyboard
  shortcuts already cover these; menu-ifying is consistency, not
  capability. Add when a user asks.
- **Audio menu (Mute / Volume / per-channel)** — Numpad shortcuts
  already cover these. Same shape as Tape; same rationale to defer.
- **Debug / Tools menu** — would showcase "observable by design"
  via inspectors for the `spectrum.basic.*` queries we just added,
  registers, ULA state. Real engineering with its own commit.
- **Window menu** — muda doesn't add it automatically; a single-
  window emulator doesn't need it.
- **rfd-accessory-view checkbox to disable autoload** — needs
  direct Cocoa bindings; F10 covers the rare case.
- **Preferences window** — when there are real preferences worth
  persisting (autoload default, video filter default, default
  scale, ROM paths), put them here.

## Next Steps

→ Implementation. Phase shape:
  1. Add `rfd` and `open` (or pick a URL launcher) to the
     emu198x-spectrum binary's deps. Extend `AppCommand` with the
     new variants. Don't wire menu items yet.
  2. File menu — three items, three rfd dialogs, three dispatch
     handlers. Disk item enabled-state queries the live runtime's
     `supports_disk_slot`. Manual smoke against shadowkeep.tap and
     a +3 .dsk.
  3. State menu — Save / Load Snapshot. Save dialog filter on
     `.sna`/`.z80`; both go through the existing session helpers.
  4. View menu — Scale 1×–4× radio + Filter None/CRT radio. Both
     dispatch existing AppCommands; the muda radio item shape is
     copied from the Machine menu.
  5. Help menu — View on GitHub + Documentation. URL strings live
     as consts in the menu module.
  6. Update `wiki/systems/spectrum/solid-status.md`: criterion 7
     flips to DONE. Headline becomes 7 done / 4 partial / 0 not
     started.
