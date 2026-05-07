# Decision: Native menu shell architecture (Track 1C)

**Status:** Design sketch. First-cut implementation lands the Machine menu only;
File / State / View are designed for in this doc but deferred to follow-up cuts.

**Drift trigger:** if you find yourself building a menu wiring pattern that
doesn't go through the central `AppCommand` channel — or proposing
`Box<dyn SpectrumMachine>` for the live machine handle, or making muda /
rfd dispatch decisions ad-hoc per menu — **stop and re-read this doc
first.** The whole point of this design is that one foundation (channel +
runtime enum) carries all four menus; bypassing it is what creates the
rework we explicitly chose to avoid.

## What 1C is

Per `wiki/systems/spectrum/solid-status.md` Track 1C: native menu bar with
**File** (Open Snapshot / Tape / Disk via `rfd`), **Machine** (variant
selector across all 8), **State** (Save / Load), **View** (window sizing
options). Wired to existing keyboard-shortcut equivalents (no duplicate
logic paths).

The first cut implements **Machine only**. File / State / View land later
on the same foundation.

## Foundational architecture

All four menus share one pathway:

```
[muda menu click] ─┐
[winit shortcut]   ├─→ AppCommand channel ─→ App processes at frame boundary
[rfd dialog reply]─┘
```

### `AppCommand` enum

Lives in `crates/emu198x-spectrum/src/main.rs` (or a sibling module).
Exhaustive list of every action the app can take, regardless of source.

```rust
enum AppCommand {
    // Machine menu
    SwitchMachine(MachineKind),
    Reset,

    // File menu (deferred but defined)
    OpenSnapshot(PathBuf),
    OpenTape(PathBuf),
    OpenDisk(PathBuf),

    // State menu (deferred)
    SaveState(PathBuf),
    LoadState(PathBuf),

    // View menu (deferred)
    SetWindowScale(u32), // 1, 2, 3, 4 → 1×/2×/3×/4× of the 352×296 framebuffer

    // Plumbing
    Exit,
}

enum MachineKind {
    Spectrum16K, Spectrum48K, SpectrumPlus,
    Spectrum128K, SpectrumPlus2,
    SpectrumPlus2A, SpectrumPlus2B, SpectrumPlus3,
}
```

### Channel

`std::sync::mpsc::channel::<AppCommand>()`. The receiver lives in the App;
every event source clones a Sender. Three event sources push commands:

1. **muda menu events** — `muda::MenuEvent::receiver()` yields `MenuEvent { id }`
   on a global channel that the winit event loop polls each iteration.
   Map `id` → `AppCommand` via a `HashMap<MenuId, AppCommand>` built at startup.

2. **winit keyboard shortcuts** — existing keyboard-shortcut paths emit
   `AppCommand` directly into the channel rather than calling runtime
   methods inline. This is what the SOLID criterion means by "wire menu
   actions to existing keyboard-shortcut equivalents (avoid duplicate
   logic paths)" — both menu and shortcut produce the same `AppCommand`,
   processed identically.

3. **rfd file-dialog replies** — the dialog opens on a worker thread
   (rfd's sync API blocks); when the user picks a path, the worker pushes
   `AppCommand::OpenSnapshot(path)` (or `OpenTape` / `OpenDisk` / `SaveState`
   / `LoadState`) back into the channel. The main loop never blocks on a
   dialog.

### Frame-boundary processing

The App's main loop, schematically:

```rust
loop {
    // 1. Drain command queue
    while let Ok(cmd) = command_rx.try_recv() {
        app.handle_command(cmd);
    }

    // 2. Drain muda receiver, translate, drain again
    while let Ok(menu_evt) = muda::MenuEvent::receiver().try_recv() {
        if let Some(cmd) = menu_action_map.get(&menu_evt.id) {
            app.handle_command(cmd.clone());
        }
    }

    // 3. Run a frame
    app.run_frame();

    // 4. Present
    app.present();
}
```

Commands run *between* frames. A `SwitchMachine` mid-frame would tear down
state the frame is using; processing at the boundary avoids that class of
bug. Same logic applies to `LoadState`.

## The live machine: enum, not trait object

The problem: each variant produces a different concrete type at compile
time (`Spectrum48kRuntime`, `Spectrum128kRuntime`, etc. are eight distinct
types from the generic `SpectrumRuntime<M>`). The app needs **one** variable
that holds "whichever machine the user is currently running." Rust requires
all match arms to produce the same type, so a literal
`let machine = match user_choice { ... }` over different concrete runtimes
won't compile.

Three ways to give it one type:

| Option | Tradeoff |
|---|---|
| `Box<dyn SpectrumMachine>` | Loses some inherent methods, dyn-dispatch overhead, requires the trait to expose every callable surface. Object-safety constraints leak into the trait. |
| `enum LiveMachine { M16K(Spectrum16kRuntime), M48K(Spectrum48kRuntime), ... }` | Verbose (8 variants × match arms), but exhaustive, type-safe, zero overhead. The variant set is fixed at 8. |
| Restart the loop with a new generic instantiation | Complex; main-loop state is non-trivial to recreate; window persistence becomes awkward. |

**Decision: enum.** The variant set is closed, the dispatch sites are small
(run_frame, framebuffer access, input dispatch, snapshot save/load), and
exhaustiveness tells us at compile time when a new variant lands and
forgets to wire one of the match arms. Could generate the enum via a
macro later if maintenance becomes painful; for 8 entries, manual is
fine. Naming is bikeshed — `LiveMachine`, `CurrentMachine`, or
`RunningMachine` all work.

Sketch:

```rust
enum LiveMachine {
    M16K(Spectrum16kRuntime),
    M48K(Spectrum48kRuntime),
    MPlus(SpectrumPlusRuntime),
    M128K(Spectrum128kRuntime),
    MPlus2(SpectrumPlus2Runtime),
    MPlus2A(SpectrumPlus2ARuntime),
    MPlus2B(SpectrumPlus2BRuntime),
    MPlus3(SpectrumPlus3Runtime),
}

impl LiveMachine {
    fn run_frame(&mut self) { match self { Self::M16K(r) => r.run_frame(), ... } }
    fn framebuffer(&self) -> &[u8] { match self { Self::M16K(r) => r.framebuffer(), ... } }
    fn kind(&self) -> MachineKind { ... }
    // etc.
}
```

A switch is simply `*self.machine = LiveMachine::new(target_kind)`. Boot
firmware load, framebuffer rebind, audio reset all happen in `LiveMachine::new`.

## Per-menu mechanics

### Machine (first cut)

- Top-level `Machine` menu with 8 radio items, current variant checked.
- Each item builds an `AppCommand::SwitchMachine(kind)`.
- `handle_command` for `SwitchMachine` constructs a fresh `LiveMachine`,
  drops the old one, updates the window title to reflect the variant.
- Default boot: 48K (current behaviour) until the first switch.
- **No keyboard shortcuts.** Menu access only — eight variants is too
  many to map to memorable shortcuts, and the menu is one click away.

### File (deferred)

- **Open Snapshot…** — rfd `AsyncFileDialog` with `["sna", "z80"]` filter,
  pushes `AppCommand::OpenSnapshot(path)` on completion. Handler dispatches
  via existing snapshot-loading code per current machine.
- **Open Tape…** — `["tap", "tzx"]` filter → `OpenTape`.
- **Open Disk…** — `["dsk"]` filter → `OpenDisk`. Item enabled only when
  the current machine reports `supports_disk_slot("disk-a") == true`
  (i.e., +3). Disabled state is muda-native; no runtime panics.

### State (deferred)

State file format: header (magic `EMU198XS\0`, version u8, machine-id
string) + postcard-serialized runtime. Snapshot infrastructure already
exists per-variant; State adds the header so Load can validate.

- **Save State…** — rfd save dialog with `.emu198xstate` filter →
  `AppCommand::SaveState(path)` → write magic + machine-id + postcard
  bytes.
- **Load State…** — rfd open dialog → `AppCommand::LoadState(path)` →
  parse header, check machine-id matches current `LiveMachine` kind. **First
  cut: error if mismatch.** Auto-switching the machine would be a bigger
  scope decision (would the user expect their unsaved work to be tossed?
  what about open tape/disk media? defer).

### View (deferred)

Window scale options (1×, 2×, 3×, 4×) translate to
`window.set_inner_size(LogicalSize::new(352*scale, 296*scale))`. **No
keyboard shortcuts** — menu access only, consistent with Machine. Trivial;
no runtime interaction.

## Out of scope for 1C

- **MCP server.** SOLID criterion 5, separate engineering effort.
  Track 1B's `--mcp` mode flag wires into the same `AppCommand` channel
  later — MCP commands become another channel sender.
- **Per-platform native frontends.** SwiftUI / GTK4 / WinUI rewrites
  were originally framed as post-October work in
  `wiki/decisions/native-ui-strategy.md`. **The 1C scope below may make
  them unnecessary** — if muda's NSMenu / GTK4 menu / Win32 menu gives a
  sufficiently native feel and rfd's dialogs satisfy file UX, the
  cross-platform muda layer might *be* the long-term frontend rather
  than a stopgap. Decision deferred until 1C lands and we can judge it
  in use; if we keep 1C as final, update `native-ui-strategy.md` to
  reflect the simplification.
- **Multi-window support.** One window per app process for now.
- **Drag-and-drop file open.** Could fold into File later via the same
  command channel — drop event becomes a sender.

## Open questions to settle while implementing

1. Does muda's macOS NSMenu integration require a specific `winit` feature
   flag or `EventLoopBuilderExtMacOS::with_default_menu(false)` call to
   suppress winit's default menu? Confirm in implementation.
2. Where does `LiveMachine` actually live in the existing code? Probably
   a new `crates/emu198x-spectrum/src/live_machine.rs` module rather than
   adding to runtime — the enum is binary-specific, not runtime-library
   surface. The runtime crate stays generic.
3. Are there any per-variant audio resampler quirks that mean a fresh
   `LiveMachine::new` needs special teardown of the cpal stream? Probably
   not — cpal stream is bound to the App, not the machine — but verify
   when we switch and the audio doesn't glitch.

## Pointer

Implementation lands in:
- `crates/emu198x-spectrum/Cargo.toml` — add `muda`, `rfd` deps.
- `crates/emu198x-spectrum/src/main.rs` — channel, command handling.
- `crates/emu198x-spectrum/src/live_machine.rs` (new) — `LiveMachine` enum.
- `crates/emu198x-spectrum/src/menu.rs` (new) — muda menu construction +
  `MenuId → AppCommand` map.

`wiki/systems/spectrum/solid-status.md` Track 1C log entry on completion.
