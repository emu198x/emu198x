# Host UI model: build it in the Mac app, then bring the other hosts level

**Decision:** `knowledge/decisions/host-ui-model.md` (approved by Steve
2026-10-25, with the GTK4 Linux frontend brought forward).
**Builds on:** `docs/plans/2026-10-20-apple-native-host-spike.md`. Its
steps 4–6 should close before step 2 here starts.

## Boundaries

- The emulation core is unchanged. Each step adds host code, catalogue
  data, query paths or bridge functions, and never machine behaviour.
- The Spectrum is the first family. Each step names what makes it work
  for a second family (the C64 is the next one the Apple build carries).
- There is no unified launcher. Each Mac app holds one family.
- A new dependency, or a new `unsafe` outside `emu198x-apple`, needs
  Steve's approval first. `gtk4` (gtk4-rs) for step 12 is approved.

Each step is one PR with a conventional title, green before the next one
starts.

## Steps

### 1. The command catalogue in `emu198x-shell`

- `crates/emu198x-shell/src/commands.rs`: a static `CATALOGUE` with id,
  label, group, a shortcut per platform, an availability key and an
  optional MCP verb. It is data only.
- Seed it from what the hosts already do: open, save state, load state,
  pause, restart, tape play and stop, show statistics, full screen,
  scale, and quit.
- `emu198x-ui/src/menu.rs` builds its `muda` menu from the catalogue in
  place of its hand-written list.
- The tape fast-forward item is labelled "Fast-Forward While Playing",
  in place of "Fast Load" (decision § 5).
- The C ABI function `emu198x_commands`, which returns the catalogue as
  JSON, moves to step 2, where the Mac app first reads it.

Verification:
- A unit test fails when two entries share an id or a shortcut on the
  same platform.
- A winit test lists every catalogue id the shell claims to support, and
  checks that each one has a menu item and an `AppCommand`. Removing a
  mapping makes it fail.
- A test checks that each catalogue shortcut is the key the winit
  shortcut handlers act on.

### 2. Mac: detail levels and the Peek status line

- The C ABI gains `emu198x_commands`, which returns the catalogue as JSON
  (`emu198x_shell::commands::catalogue_json`). Header and symbol tests
  are updated, and `swift test` decodes the JSON and checks it against
  the catalogue. The Mac menus are built from it.
- The Mac app's own shortcuts (⌘P pause, ⇧⌘R restart, ⌘T tape) and the
  baseline's (F9–F12) differ today. Settle them in the catalogue here;
  this step adds pause and restart as catalogue entries.

- A View menu item for each of Play, Peek, Inspect and Debug (⌘1–⌘4),
  generated from the catalogue.
- Play: the toolbar hides, and controls appear when the pointer moves.
- Peek: a status line with frame rate, audio buffer and underruns (from
  `EmulatorLoop`), tape block and progress (`tape.block`, `tape.blocks`,
  `tape.progress`), joystick port, and scale.
- `ScreenView` drops to the next whole-number scale when panels take
  space.

Verification: a capture at backing resolution for each level, showing
a whole-number scale and even pixels. The level is restored after
relaunch. The audio underrun count does not change while switching
levels for a minute.

### 3. Mac: the Welcome window, document types and drop to play

- A Welcome window listing the family's variants with the firmware status
  of each, checked by SHA-1 before boot. Choose ROM and Show ROM Folder
  open `~/.emu198x/roms/`.
- Document types for `.tap`, `.tzx`, `.sna`, `.z80`, `.szx` and
  `.emu198x`. Double-clicking one opens the app. Recents use
  `NSDocumentController`.
- Drop a file on the window to play it. Autoload is shown, and on by
  default for tapes.

Verification: open each type from the Finder. A missing ROM and a wrong
ROM each show their own message in the Welcome window, never a crash.

### 4. Mac: the media deck

- A toolbar control at Peek and above, and a section of the pause sheet:
  counter, Play/Stop, previous and next block, and a block list from
  `tape.blocks`.
- Fast-forward while the tape plays: `EmulatorLoop` advances the machine
  faster than real time with sound muted, and returns to real time when
  `tape.playing` goes false. No loader shortcut.

Verification: Dizzy loads with fast-forward on, and its frame hash at
its title matches a load at real speed after the same number of machine
frames. Audio returns within one frame of the tape stopping.

### 5. Mac: Controls

- Bring tvOS `ControlsView` to the Mac as a window. `ControllerMap` and
  `GameControls` are already shared.
- Show the Spectrum keyboard at its real layout, highlighting the keys
  the preset uses.
- A choice between mapping keys by position and by symbol. Mapping by
  symbol needs a table per family of host characters to guest chords.
- ⌘Esc releases the keyboard to the host.

Verification: the twelve Dizzy games start from a controller alone on
the Mac, using the presets and select keys found in the spike. Typing
`"` by symbol reaches the Spectrum as Symbol Shift + P.

### 6. One save format, slots and thumbnails

Needs the format question in the decision settled first.

- The Mac app reads and writes the browser's `.emu198x` file.
- Six slots for each game, each with a thumbnail of its frame, and
  autosave on quit.
- Restoring a save clears the audio ring (the spike's finding in step 3).

Verification: a save from the browser player restores in the Mac app,
and a save from the Mac restores in the browser player. Frame hashes
match after 100 frames.

### 7. Rewind

- A ring of snapshots at a fixed number of frames apart, held in the
  runtime host behind `Fleet`, with a frame limit. Bridge functions
  `rewind` and `rewind_depth`.
- On the Mac: hold ⌘← to rewind, a scrubber in the pause sheet, and frame
  step with ⌘[ and ⌘].

Verification: rewind 10 s, then replay with the same input, and the frame
hashes match the original run. Memory use is measured and recorded here.
Holding rewind does not drop audio below real time once the hold is
released.

### 8. Mac: Inspect

- A side panel at the Inspect level: Z80 registers (`cpu.*`), ULA
  scanline, T-state and border, tape, and a one-line explanation where
  a query names the routine.
- A full Inspector window: CPU, memory, ULA, beeper, keyboard matrix,
  tape signal, system variables and BASIC listing.
- The memory view needs memory reads, as a query path or a bridge
  function (open question in the decision).
- The memory view shows where a screen byte lands on the picture.

Verification: with the machine paused, every value shown matches the
same query over the MCP server. Values refresh every frame without
changing the audio underrun count.

### 9. Saying what's true

- A guest halt (`cpu.halted` with interrupts off) shows a sheet that
  explains it, with Rewind, Inspect and Restart.
- A badge for replacement firmware, once a family ships one.
- Accuracy per variant, read from the status pages CI renders.
- Tapes that are only partly readable get a dialog that lets the user
  carry on.

Verification: a test program that runs `DI` then `HALT` brings up the
sheet. A TZX with an unsupported block brings up the partial-load dialog
naming that block.

### 10. Debug

- First: pause, step and a disassembly strip, through `DebugTarget`.
- Then: a native SwiftUI debugger window as an MCP client over
  `--mcp-listen` (decision § 9).

### 11. Bring the winit baseline level

- Menus from the catalogue (from step 1).
- The Peek status line and toasts, drawn within the limits in the
  decision's section 10.
- Controller presets from `<name>.controls.json`, using the same file
  the Mac and TV read.
- The `.emu198x` save format from step 6.
- Linux gets its menu from step 12, not from this step.

Verification: each catalogue id the baseline supports passes the step 1
test. A save and a controls file made on the Mac work unchanged on the
baseline.

### 12. Linux: the GTK4 frontend

Approved 2026-10-25 (decision § 12). It can start in parallel with steps
2–10, once step 1 has landed, because it reads the same catalogue.

- First, a refactor with no behaviour change: the window-free half of the
  winit app becomes `HostCore` (`emu198x-ui/src/host_core.rs`), which a
  window drives through a small `Shell` trait. Steve chose this over a GTK
  host with its own loop, which would have copied pacing, commands and
  media handling (#2114).
- The GTK window is a program of its own, `emu198x-gtk-window`
  (`crates/emu198x-gtk-window`), so the machine binaries never link GTK and
  their headless, script and MCP modes run where GTK is not installed
  (`knowledge/decisions/gtk-window-process.md`). The machine binary keeps
  `HostCore`, starts the window beside its executable, and talks to it over
  its standard input and output (`emu198x-ui/src/window_protocol.rs`). With
  no window to start, or one that fails, it opens the winit window;
  `EMU198X_UI=winit` picks the winit window. (It began inside `emu198x-ui`
  behind a `gtk` feature, #2117.)
- `GtkApplication` with its menu bar built from the catalogue as a
  `gio::Menu`. Each item is a GTK action that sends the same `AppCommand`
  the shortcuts do. Radio and check items are stateful actions whose marks
  the core moves after a command takes effect.
- The picture is a GTK widget that draws each frame as a texture, at the
  machine's pixel aspect and the largest whole-number scale that fits,
  with nearest-neighbour filtering. GTK keycodes are translated into
  winit's `KeyCode`, so every `UiSystem` key map works unchanged.
- The host is written against GTK 4.6, so it runs on Ubuntu 22.04 (the
  release builders' base, with 4.6) and Debian 12 (4.8). File choosers are
  `GtkFileChooserNative`, which uses the desktop portal where there is one,
  and messages are `GtkMessageDialog`. Every picture reaches GTK already
  composed at device pixels, so GTK never scales it.
- The `gtk4` dependency and the system `libgtk-4-dev` it needs are
  confined to the window crate's `gtk` feature. A `GTK host` CI job lints
  and tests it.
  `muda` stays stubbed on Linux, so the gtk3-rs tree and its Dependabot
  alert never come back.
- The picture comes from the same wgpu presenter as the winit window,
  rendering offscreen at the picture's device-pixel size (its
  `new_offscreen` mode), for every filter including raw. Without a GPU
  adapter the host draws the raw frame with GTK's nearest-neighbour
  scaling. The CRT picture is pixel-identical to the winit window's under
  Xvfb.
- Raw uses sharp bilinear (decision § 4) in the presenter's shader, so both
  desktop hosts get it: exact nearest-neighbour at whole-number scales, and
  only the output pixels a block edge crosses blended at fractional ones.
- Menu items show the catalogue's shortcuts through the menu model's
  `accel` attribute, which only labels them: the keys still go to the core,
  so a machine that maps F9 to F11 keeps them.
- Linux opens GTK by default: every Linux release archive carries the
  window in `libexec/`, and Homebrew installs it under the formula's
  `share/` directory. It uses the system's GTK; the formulae declare no GTK
  dependency, because dist cannot limit one to Linux and a prebuilt binary
  would not find Homebrew's GTK anyway
  (`knowledge/decisions/gtk-window-process.md`, "How it ships").

Verification:
- The catalogue test from step 1 passes for the GTK host.
- Under Xvfb, the ZX81 with the synthetic ROM opens in the GTK window with
  the same picture as winit, and Ctrl+S, Ctrl+L and F9 give the same log.
- Dizzy loads and plays at 50 Hz with no audio underruns over 10 minutes
  (needs a desktop with sound).
- `cargo tree` shows no gtk3 crate anywhere in the workspace.
- `ldd` on a machine binary lists no GTK library, and `--headless` runs
  with no display. With the window missing, or unable to start GTK, the
  machine opens the winit window.
- The Linux package test installs each generated formula and checks that
  `share/<formula>/libexec/emu198x-gtk-window` is there and links GTK 4.

## Not in this plan

- Shipping or distributing Apple builds, and the firmware policy for them.
- The WinUI frontend for Windows, which starts once the Mac app has
  settled (decision § 1).
- The tvOS changes, which follow each Mac step as the Mac app's TV
  variant rather than as steps of their own.
- A library or store browser beyond recents.
