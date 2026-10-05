# Decision: the application icon is set at runtime, plus a Windows resource

**Date:** 2026-10-05
**Status:** Active. Governs how the desktop emulators show the Emu198x icon.

## The decision

The release ships bare binaries in archives, not `.app` bundles or Linux
packages, so the icon has to come from the binary itself:

| Where | How | Code |
|---|---|---|
| Window icon (Windows title bar and taskbar, Linux X11) | winit window attributes, from PNGs embedded in `emu198x-ui` | `crates/emu198x-ui/src/icon.rs` |
| macOS Dock, while running | an `NSImageView` set as the Dock tile's content view | same file |
| Windows `.exe` in Explorer | an `.ico` resource compiled by `embed-resource` | `crates/emu198x-app-icon`, called from each emulator's `build.rs` |
| Linux `.desktop` entry | `emu198x.png` shipped in every release archive | `include` in `[workspace.metadata.dist]` |

An icon that fails to load is a warning, never a startup failure.

## Why the Dock tile, not `applicationIconImage`

`NSApplication.setApplicationIconImage` is `unsafe` in objc2-app-kit 0.3, and
the workspace sets `unsafe_code = "forbid"`, which no crate can relax with an
`allow`. The Dock tile's content view is reachable through safe bindings.

The Cmd-Tab switcher shows the tile as well: checked by hand on the notarised
v0.26.0 `emu198x-spectrum` on Apple silicon, so the switcher needs no `unsafe`
call. System alerts are unchecked. If one ever shows the generic icon, the fix
is a single `unsafe` call and a crate-level lint exception, which is a
separate decision.

The objc2 crates are the 0.6/0.3 line that muda and rfd already pull in.
winit 0.30 uses the older 0.5/0.2 line, whose API is unsafe throughout, so
matching winit would have meant a lint exception too.

## Why a shared build helper

Thirty emulator binaries need the same resource. Each `build.rs` is one call
into `emu198x-app-icon`, which does nothing unless `CARGO_CFG_TARGET_OS` is
`windows`, so macOS and Linux builds need no resource compiler. A cross build
without `llvm-rc` warns and builds without the file icon; a resource compiler
that runs and fails stops the build.

## Not covered

- **macOS Finder icon.** Needs an `.app` bundle with an `.icns`. Separate work.
- **Wayland.** Clients cannot set a window icon; the compositor uses the
  `.desktop` entry whose `Icon=` names `emu198x.png`.
- **The browser player.** A favicon on the standalone player page would be
  the equivalent; not done here.

## Drift triggers

- Adding a `build.rs` to an emulator binary that does more than call
  `emu198x_app_icon::embed()`.
- Copying icon PNG bytes into a machine crate rather than `emu198x-ui`.
- Editing the icon SVGs by hand instead of regenerating them from the 198x-ui
  kit (`crates/emu198x-app-icon/art/README.md`).
