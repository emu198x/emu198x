# The GTK window runs in its own process

**Status:** Adopted 2026-10-26 (Steve asked for headless modes to stop
depending on GTK). Applies to the Linux GTK4 frontend of
[`host-ui-model.md`](host-ui-model.md) § 12. The window lives in
`crates/emu198x-gtk-window`; its protocol is
`crates/emu198x-ui/src/window_protocol.rs`.

## The problem

The GTK host was first built into `emu198x-ui` behind a `gtk` feature, so
a machine binary built with it linked `libgtk-4`. A binary linked against
GTK does not start where GTK is not installed: not for the window, and
not for `--headless`, `--script` or `--mcp` either, which never draw
anything. A server, a CI runner or a container would need the whole GTK
stack to run a script. Turning the feature on by default would have made
that true of every Linux release.

## The decision

The machine binaries never link GTK. On Linux the window is a separate
program, `emu198x-gtk-window`, that the machine binary starts when it
needs a window:

- The machine binary keeps the machine: `HostCore`, pacing, audio,
  commands and media. The window process holds only GTK, the picture
  widget, the menu bar and the wgpu presenter that composes each frame.
- They talk over the child's standard input and output, in the messages of
  `window_protocol.rs`: a little-endian `u32` length, then postcard.
  Frames, title and size requests, menu marks, cursor visibility and
  dialog requests go to the window; input, menu commands, dialog answers
  and closing come back. The core's `Shell` trait is implemented by
  sending these messages.
- The first message carries `PROTOCOL_VERSION`. A window that speaks
  another version answers `Incompatible` and exits.
- The machine binary looks for the window at `EMU198X_GTK_WINDOW`, then
  beside its own executable, then in `libexec/` beside it, then in
  `share/<binary>/libexec/` under its prefix. If the window is missing,
  will not start, is incompatible or does not say `Ready` within 15
  seconds, the machine binary opens its built-in winit window, as it did
  before. `EMU198X_UI=winit` skips the GTK window.
- If the machine binary goes, the window sees its input close and shuts.
  If the user closes the window or picks Quit, the window sends `Closed`
  and the machine binary ends the session as it does for the winit window.

## How it ships

Every Linux release archive carries the window as
`libexec/emu198x-gtk-window`, beside the machine binary. The release job
builds it there before dist packs the archives
(`scripts/build-gtk-window.sh`). dist's `include` list is the same for
every target, so the macOS and Windows archives carry `libexec/` empty.

Homebrew installs the archive's extra files into the formula's
`share/<formula>/` directory rather than `bin/`. That is where the machine
binary finds the window, and it means the thirty formulae never collide
over one `bin/emu198x-gtk-window`.

The window uses the system's GTK, 4.6 or later, and the formulae declare
no GTK dependency. dist can only declare a Homebrew dependency for every
platform, which would install GTK on macOS too. On Linux it would not help
either: the prebuilt window has no search path into Homebrew's prefix, so
it would not find a GTK that Homebrew installed. GNOME desktops have GTK 4
already. Where it is missing, the window cannot load, and the machine
binary says so and opens the winit window.

## Why

- **Headless modes need nothing they do not use.** `ldd` on a machine
  binary lists no GTK, GDK, GLib, cairo or pango library, so `--script`
  and `--headless` run on a system with no GTK at all.
- **The machine stays in one process.** The window is a view: it holds no
  machine state, so a window that crashes loses no work the machine
  binary cannot save, and every `UiSystem` runs unchanged.
- **One window serves all thirty binaries.** Each machine binary opens it
  through `emu198x_ui::launch`, so no machine crate depends on GTK or on
  the window crate.

The cost is a copy of each frame through a pipe: a few hundred kilobytes,
fifty or sixty times a second. When frames arrive faster than the window
draws them, it draws only the latest.

## What this is not

It is not a launcher under [`no-unified-launcher.md`](no-unified-launcher.md).
The window process cannot be run on its own to pick or start a machine: it
opens only when a machine binary starts it, and shows only that machine.
Each machine binary is still the product.

## Drift triggers

Stop and re-read this record if you are about to:

- **Make a machine crate, `emu198x-shell` or `emu198x-ui` depend on
  `gtk4`.** The point of the split is that they do not.
- **Move machine state into the window process,** or give it a way to
  start a machine of its own.
- **Change a message without bumping `PROTOCOL_VERSION`,** once a release
  has shipped the window.
- **Add a Homebrew `depends_on "gtk4"`.** It would install GTK on macOS as
  well, and the prebuilt window would not find it on Linux.
- **Install the window into a formula's `bin/`.** Thirty formulae would
  collide over the one name.
- **Print to standard output in the window process.** Standard output is
  the protocol; diagnostics go to standard error.

## Related

- [`host-ui-model.md`](host-ui-model.md) § 12: the GTK4 frontend
- [`machine-binaries-share-a-launcher.md`](machine-binaries-share-a-launcher.md):
  how every binary reaches its window
- [`no-unified-launcher.md`](no-unified-launcher.md)
- [`native-ui-strategy.md`](native-ui-strategy.md)

## Log

| Date | Event |
|------|-------|
| 2026-10-26 | Adopted. Steve asked for headless modes not to depend on GTK; the window moved out of `emu198x-ui` into its own process. |
| 2026-10-26 | Shipped in the Linux release archives, on by default (Steve). Uses the system's GTK; no Homebrew GTK dependency. |
