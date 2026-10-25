# Decision: One host UI model, with the Mac app as its reference

**Date:** 2026-10-25
**Status:** BINDING. Approved by Steve 2026-10-25, with section 9's
amendment to [`debugger-architecture.md`](debugger-architecture.md) and a
GTK4 Linux frontend (section 12).
**Mockups:** the "Emu198x macOS UI" design canvas (six screens), made
alongside this record.
**Plan:** [`../../docs/plans/2026-10-25-host-ui-model.md`](../../docs/plans/2026-10-25-host-ui-model.md).

## The problem

Emu198x now has four hosts: the winit baseline, the Mac app, the Apple TV
app and the browser player. Each grew its own commands, its own way of
showing a tape and its own idea of what a save is. Some of the results:

- The Mac app's save is the runtime's raw state bytes. The browser's is a
  `.emu198x` file. Neither can open the other's
  (`2026-10-20-apple-native-host-spike.md`, step 4).
- Controller presets with select keys exist only on Apple TV. Every
  desktop Spectrum user who wants to play Dizzy with a pad meets the
  problem the TV work solved.
- `muda` is stubbed on Linux, so Linux users have shortcuts but no menu.
- [`native-ui-strategy.md`](native-ui-strategy.md) rejects egui for the
  product, but [`debugger-architecture.md`](debugger-architecture.md) § 6
  chose egui for the debugger.

[`native-ui-strategy.md`](native-ui-strategy.md) says what each host is
built *with*. Nothing yet says what each host *does*. This record says
that.

## The decisions

### 1. The Mac app is the reference host

Each design question is answered first in `apple/Emu198x` (macOS target).
The other hosts match its **behaviour**: the same commands, levels, media
handling, input model and save format. They do not copy its **look**,
which belongs to each platform under
[`native-ui-strategy.md`](native-ui-strategy.md).

Why the Mac: it is the first true native frontend, it is the platform the
owner uses every day, and SwiftUI gives every part of this model a native
widget (sheets, inspectors, a Settings window, document types).

### 2. Four detail levels, one at a time

| Level | Shortcut | Shows | Data |
|---|---|---|---|
| **Play** | ⌘1 | The picture only. Controls appear when the pointer moves and fade away. | none |
| **Peek** | ⌘2 | A one-line status bar with speed, audio buffer, media position, joystick port and scale. The toolbar shows the media deck. | loop statistics; `tape.*` queries |
| **Inspect** | ⌘3 | A native side panel with the CPU, the video chip, media, and a plain-English line on what the machine is doing. Opens into the full Inspector window. | `cpu.*`, `tape.*`, chip queries through the query provider |
| **Debug** | ⌘4 | Pauses. Shows disassembly with step controls and opens the debugger window. | `DebugTarget` through MCP (section 9) |

Choosing a level never changes how the machine runs, except Debug, which
pauses. The picture keeps its size rule (section 4) at every level; when
the panels take space, the scale drops to the next step.

These levels are how one app serves a player, a learner and a developer.
The default is Play, and each level only adds to the one before it.

### 3. One command catalogue

`emu198x-shell` gains a **command catalogue**: data only, no behaviour.
Each entry has:

- a stable id, such as `media.tape.play`;
- a label and a menu group;
- a shortcut for each platform;
- an availability key, such as "needs a running machine" or "needs a tape
  slot";
- the MCP verb that does the same thing, if there is one.

Every host builds its menus, shortcut help, toolbar and tvOS pause menu
from the catalogue. The host's own command enum (`AppCommand` in the winit
shell, `HostCommand` in `Emu198xKit`) still does the work. The catalogue
only describes it. So [`native-menu-shell.md`](native-menu-shell.md)'s
single command path stays as it is, and this sits in front of it.

`emu198x_shell::control::ControlCommand` is the seed: it already gives
operations stable names (`media-transport`).

A test in each host checks that every catalogue id it claims to support
appears in its menus. Adding a command to the catalogue without wiring it
up fails that host's build.

### 4. One rendering default

Every host uses the rule the Mac app settled on 2026-10-20:

- the true pixel aspect, from `pixel_aspect_ratio`;
- sharp bilinear, where each source pixel is a solid block and only the
  edges between blocks are blended;
- a whole-number scale when it fits.

CRT and other filters are choices a user makes on top of that default.
The Dragon opening on CRT (`UiSystem::default_video`) is a machine-chosen
default, which stays allowed. It must be named in that machine's record.

### 5. Media are devices

Each media slot appears as the device it stands for:

- a tape deck with a counter, Play/Stop, previous and next block, and the
  block list;
- a drive with its activity light and track;
- a cartridge slot.

Loading at real speed is the default.

**Fast-forward** runs the whole machine faster, with sound muted, until
the tape stops. It never takes a shortcut past the loader, so it stays
within [`no-rom-trap-load.md`](no-rom-trap-load.md). The interface names
it "fast-forward", never "instant load".

Autoload (type `LOAD ""` and press Play) is a choice the user can see, as
it already is on tvOS and in the browser.

### 6. Input

- **Per-game controller presets**, as on tvOS: Kempston, Sinclair, Cursor,
  Keys, and select keys or key sequences. They are stored by file name,
  and `<name>.controls.json` beside a game can supply them.
  `Emu198xKit`'s `ControllerMap` stays machine-neutral. Each family
  provides its own list of presets.
- **Host keys never reach the guest.** ⌘ shortcuts on the Mac, and a fixed
  host modifier on other platforms. A shortcut that releases the keyboard
  (⌘Esc) is always available.
- **Keys map by position by default**, with mapping by symbol as an
  option. Typing `"` by symbol gives Symbol Shift + P on a Spectrum.
- **The on-screen keyboard is the machine's real layout**, and highlights
  the keys the current preset uses.

### 7. Time

- Pause, frame advance, and a rewind ring built on the serde save-state
  machinery [`save-state-format.md`](save-state-format.md) already plans
  for.
- Save slots with thumbnails.
- **One save file format on every host.** The browser player's `.emu198x`
  file becomes the shared format, unless that record says otherwise. A
  save made in any host opens in any other.

### 8. Say what's true

- When the guest stops the machine (`DI` then `HALT`, or a 6502 JAM), the
  app says so in the guest's own terms. It explains that a real machine
  would do the same, and offers Rewind, Inspect and Restart. This extends
  the existing halt overlay.
- **Replacement firmware** (Open ROMs, AROS) shows a badge that stays for
  the whole session and is written into saves and bug reports. This
  follows the firmware note in the Apple spike plan.
- **The accuracy of a variant** is read from the status pages CI renders
  (`emu198x/docs` status). It is never typed into the app.
- **A partly readable file** says which blocks loaded and lets the user
  decide whether to carry on.
- Every ROM is checked against its known dump before it boots. A bad dump
  is reported by name.

### 9. The debugger follows its host

**Amends [`debugger-architecture.md`](debugger-architecture.md) § 6** (approved 2026-10-25).

That record's choices 1–5 and 7–10 stand: a separate process, an MCP
client over `--mcp-listen`, several clients at once, learner-first, and
the MVP scope. Choice 6, egui, conflicts with
[`native-ui-strategy.md`](native-ui-strategy.md), which rejects egui for
anything shipped to users.

The debugger is a native window on each host, built from the same
toolkit as that host's app (SwiftUI on the Mac, GTK4 on Linux). It stays
an MCP client, so it can still run as a separate process and sit beside
Claude.

The Debug level (section 2) ships the pause, step and disassembly strip
first, which `Fleet` and `DebugTarget` can already serve.

### 10. What the winit baseline may draw itself

The baseline has no native widgets in the window, so the line has to be
stated. It may draw only:

- the halt overlay;
- one-line toasts that dismiss themselves;
- a dimmed scrim while paused;
- the Peek status line.

Nothing it draws takes clicks. Anything interactive goes through `muda`
menus, `rfd` dialogs, or a native window.

### 11. Accessibility

- Every control the app draws has a label a screen reader can read. The
  guest's picture is one labelled image.
- Every host binding can be rebound.
- Reduced Motion turns off CRT flicker and other animated filters.
- Status is never shown by colour alone.

### 12. Linux gets a GTK4 frontend now

Approved by Steve 2026-10-25. The `muda` stub on Linux was a defect, not
a choice: `muda`'s Linux backend is GTK3, whose gtk3-rs bindings carry the
Dependabot alert, so the menu was dropped rather than replaced. Linux
moves to a GTK4 frontend through `gtk4-rs`, the post-launch path in
[`native-ui-strategy.md`](native-ui-strategy.md), brought forward. It
calls GTK's own widgets, so it is a native frontend under that record,
not a cross-platform toolkit. Until it lands, Linux keeps the shortcuts.

## Drift triggers

Stop and re-read this record if you find yourself:

- adding a menu item, shortcut or pause-menu entry that isn't in the
  catalogue;
- giving one host a save format another host cannot read;
- writing "instant load" or adding a loader shortcut to make tapes faster;
- drawing an interactive control into the picture in the winit baseline;
- adding egui for the debugger before section 9 is settled;
- typing accuracy claims into the app instead of reading them from the
  status pages;
- defaulting a host to a filter other than sharp at the true aspect
  without naming it in that machine's record.

## Open questions

- **Save format.** The browser's `.emu198x` file is the shared format
  (approved 2026-10-25). How it carries the `EMU1` header from
  `save-state-format.md` is settled in plan step 6.
- **The Inspector's memory view** needs memory reads through `Fleet`. Use
  the query provider (`memory.read`) or a new bridge function?
- **The per-machine app.** Each Mac app is a single family, under
  [`no-unified-launcher.md`](no-unified-launcher.md). The Welcome window
  picks a variant within the family, never another family.

## Related

- [`native-ui-strategy.md`](native-ui-strategy.md): what hosts are built with
- [`native-menu-shell.md`](native-menu-shell.md): the single command path
- [`no-unified-launcher.md`](no-unified-launcher.md): one app per family
- [`machine-binaries-share-a-launcher.md`](machine-binaries-share-a-launcher.md)
- [`debugger-architecture.md`](debugger-architecture.md)
- [`debug-surface-tiers.md`](debug-surface-tiers.md)
- [`save-state-format.md`](save-state-format.md)
- [`no-rom-trap-load.md`](no-rom-trap-load.md)
- `docs/plans/2026-10-20-apple-native-host-spike.md`

## Log

| Date | Event |
|---|---|
| 2026-10-25 | Proposed, with mockups and a plan. |
| 2026-10-25 | **Approved** (Steve). Section 9 amends `debugger-architecture.md` § 6; section 12 brings the GTK4 Linux frontend forward; `.emu198x` is the shared save format. |
