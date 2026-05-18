# Decision: Native menu shell architecture (Track 1C)

**Status:** Machine menu fully wired (Phase 1 + Phase 2 landed 2026-05-08).
File / State / View are designed for in this doc but deferred to
follow-up cuts. The trait+factory infrastructure required by those menus
(`LiveSpectrumRuntime`, `build_runtime`) is in place, so each is a
small per-menu task rather than another foundation pass.

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

## The live machine: trait + blanket impl + `Box<dyn>`

The problem: each variant produces a different concrete type at compile
time (`Spectrum48kRuntime`, `Spectrum128kRuntime`, etc. are eight distinct
types from the generic `SpectrumRuntime<M>`). The app needs **one** variable
that holds "whichever machine the user is currently running." Rust requires
all match arms to produce the same type, so a literal
`let machine = match user_choice { ... }` over different concrete runtimes
won't compile.

The first sketch of this doc proposed an `enum LiveMachine` with 8 arms.
That was the right answer in the abstract but the wrong abstraction for
the binary's actual usage pattern: `SpectrumRunner` touches the runtime
in ~30 places (time, run_until, command, audio_controls, audio
mutation methods, reset, profile, etc.), and an enum requires every
method to be an 8-arm match where every arm dispatches identically —
~240 lines of match boilerplate carrying zero information.

The trait + blanket impl approach is much cleaner here:

| Option | Tradeoff |
|---|---|
| `enum LiveMachine { M16K(Spectrum16kRuntime), M48K(Spectrum48kRuntime), ... }` | Compile-time exhaustive over the closed variant set, but ~240 lines of match boilerplate where every arm dispatches identically — zero information per arm. |
| `Box<dyn LiveSpectrumRuntime>` with a trait + single blanket impl over `SpectrumRuntime<M>` | One blanket impl covers all 8 variants; adding a new variant doesn't touch this code at all. Dyn-dispatch overhead is negligible (per-frame call sites, not cycle-level). |
| Restart the loop with a new generic instantiation | Complex; main-loop state is non-trivial to recreate; window persistence becomes awkward. |

**Decision: trait + blanket impl + `Box<dyn LiveSpectrumRuntime>`.**

```rust
trait LiveSpectrumRuntime {
    fn time(&self) -> u64;
    fn run_until(&mut self, target: u64, host: &mut HostIo) -> Result<RunResult, MachineError>;
    fn command(&mut self, cmd: &ControlCommand) -> Result<(), MachineError>;
    fn audio_controls(&self) -> &AudioControls;
    fn set_audio_controls(&mut self, controls: AudioControls);
    fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool);
    fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32);
    fn reset(&mut self, kind: ResetKind);
    fn profile(&self) -> &Profile;
    fn machine_kind(&self) -> MachineKind;
    // … exact surface emerges from refactoring SpectrumRunner
}

impl<M: SpectrumMachine> LiveSpectrumRuntime for SpectrumRuntime<M> {
    fn time(&self) -> u64 { SpectrumRuntime::time(self) }
    // … one block, covers all 8 variants
}
```

The blanket impl is itself exhaustive over the bound — there's nothing
to enumerate when one block covers everything matching `M:
SpectrumMachine`. A new variant is automatically picked up. The trait
lives in a binary-local module (`live_machine.rs`) since this is a
binary-side concern, not a runtime-library surface.

A factory function constructs a fresh boxed runtime per variant:

```rust
fn build_runtime(kind: MachineKind, firmware: &FirmwareSet)
    -> Result<Box<dyn LiveSpectrumRuntime>, AppError>
{
    Ok(match kind {
        MachineKind::Spectrum16K => Box::new(Spectrum16kRuntime::from_firmware(firmware)?),
        MachineKind::Spectrum48K => Box::new(Spectrum48kRuntime::from_firmware(firmware)?),
        // ... 6 more
    })
}
```

This is the only place where the closed variant set surfaces — one match
arm per variant, each constructing the right concrete runtime. After this
the binary works through the trait object.

A switch becomes `*self.runtime = build_runtime(target_kind, firmware)?`.
Audio reset and any other host-side teardown happens at the App level
since those resources aren't bound to the runtime.

## Per-menu mechanics

### Machine (first cut)

- Top-level `Machine` menu with 8 radio items, current variant checked.
- Each item builds an `AppCommand::SwitchMachine(kind)`.
- `handle_command` for `SwitchMachine` constructs a fresh boxed runtime
  via `build_runtime`, drops the old one, updates the window title to
  reflect the variant.
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
  parse header, check machine-id matches current runtime's `machine_kind()`. **First
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
2. Are there any per-variant audio resampler quirks that mean a fresh
   `build_runtime` needs special teardown of the cpal stream? Probably
   not — cpal stream is bound to the App, not the runtime — but verify
   when we switch and the audio doesn't glitch.

## Pointer

Implementation lands in:
- `crates/emu198x-spectrum/Cargo.toml` — add `muda`, `rfd` deps.
- `crates/emu198x-spectrum/src/main.rs` — channel, command handling.
- `crates/emu198x-spectrum/src/live_machine.rs` (new) — `LiveSpectrumRuntime`
  trait, blanket impl over `SpectrumRuntime<M>`, and `build_runtime`
  factory.
- `crates/emu198x-spectrum/src/menu.rs` (new) — muda menu construction +
  `MenuId → AppCommand` map.

`wiki/systems/spectrum/solid-status.md` Track 1C log entry on completion.

## Log: 2026-05-18 — muda GTK backend disabled at the workspace level

Dependabot raised a moderate alert (#1) on `glib 0.18.5`
(`VariantStrIter` unsoundness, patched in 0.20.0). The vulnerable
crate reached us transitively through `muda 0.16 → gtk 0.18 → glib
0.18`. gtk3-rs is in maintenance mode and won't be updated to glib
0.20, so the alert has no in-place fix on the gtk3 line.

muda 0.19 made its GTK backend optional behind a feature flag
(`default = ["libxdo", "gtk"]`, both disable-able). Because 1C's
macOS install is the only platform whose `init_for_nsapp` is wired —
the Linux / Windows `init_for_*` calls remain TODOs in
`crates/emu198x-spectrum/src/ui/menu.rs:222-227` — we can disable
muda's `gtk` feature with no functional cost today. The workspace dep
now reads:

```toml
muda = { version = "0.19", default-features = false }
```

Effects:
- Drops glib / gtk3 / cairo-rs / atk / gdk / gio / pango / libxdo and
  ~60 other transitive crates from `Cargo.lock` (audit surface ↓).
- Closes Dependabot alert #1.
- Unblocks `toml` / `toml_datetime` / `toml_edit` updates that the
  glib subtree's `proc-macro-crate 2.0.2` pin was holding back.
- Removes the `libgtk-3-dev` and `libxdo-dev` Linux CI installs.
- Windows menus are unaffected — muda uses `windows-sys` there, not
  GTK; still gated on track-1c's `init_for_hwnd` wiring.

Re-enable the feature (or switch to a hypothetical muda gtk4
backend) when we actually wire Linux native menus. Per the "Out of
scope for 1C" note above, the alternative is a per-platform
frontend, in which case the muda Linux backend may never need
re-enabling at all.
