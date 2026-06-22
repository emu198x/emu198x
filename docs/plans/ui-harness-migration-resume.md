# emu198x-ui harness — migration + new-UI resume plan

**Status as of 2026-06-22.** Bringing every runner onto the shared
`emu198x-ui` harness (tracking #561) **and** giving the ~21 headless-only
systems their first native UI (#460). Decision (Steve): do the migrations and
all the new UIs; **lift the Spectrum machinery into the harness first** to set
the canonical model; **one PR per system**; auto-merge each when CI is green.

## Done (merged)

- **#594** NES → harness. Added the `handle_key` (per-system shortcuts) and
  `on_exit` (teardown) hooks.
- **#595** Game Boy → harness. Added the fatal-error-over-teardown precedence.
- **#596** Harness **general keyboard input** (`UiSystem::map_keys(code) ->
  Option<&[&str]>`, multi-name combos, modelled on Spectrum `ui/input.rs`) +
  the **ZX81's first UI** (validator — simplest keyboard-only computer).
- **#597** ZX80's first UI (sibling of ZX81).
- **#600–#607** all of **list A** — the eight keyboard-only headless systems'
  first UIs, each one PR on the proven recipe: Jupiter Ace (#600), Mattel
  Aquarius (#601, keyboard + hand-controller — first dual-input consumer),
  Oric-Atmos (#602), Memotech MTX (#603), Tatung Einstein (#604), Acorn Atom
  (#605), Sord M5 (#606), Spectravideo SVI-328 (#607). Pattern settled across
  these: keyboard via `map_keys` mirroring each runtime's key-name scheme; where
  a machine's cursor keys are real matrix cells they type (Oric/MTX/SVI), and
  the joystick is reached by a real gamepad through `button_map`; the machine's
  own Escape stays unmapped because the harness owns Esc for quit. No harness
  changes were needed for any of them.
- **#609–#618** all of **list B** — the ten joystick-led / console-like headless
  systems' first UIs: Sega Master System (#609), Sega SG-1000 (#610),
  ColecoVision (#611), Atari 5200 (#612), Atari 7800 (#613), MSX (#614),
  Commodore PET (#615), VIC-20 (#616), Acorn BBC Micro (#617), Acorn Electron
  (#618). Patterns that settled: the pad goes through the console path
  (`map_key` → `HostControl` → `button_map`) with arrows + Z/X; a single
  console button (SMS/SG-1000 Pause, 7800/5200 Select/Reset/Pause, the Coleco /
  5200 numeric keypad) is a *named key event* via `map_keys`, kept disjoint from
  the pad host-keys so nothing double-routes; a `Region`/`Variant` field on the
  System struct carries NTSC/PAL (and Game Gear) to pick model + frame budget +
  refresh + display aspect + the per-machine Pause/Start name. Keyboard+joystick
  machines (MSX/VIC-20) reuse the list-A rule (cursor keys type, joystick =
  gamepad). The BBC best-effort installs BASIC into sideways bank 15 + the
  SAA5050 font so it boots to a prompt. Still no harness changes needed.

(Plus the unrelated Atari 2600 Supercharger work: #588, #593.)

> **Boot-verification debt (deferred, agreed with Steve):** the per-system
> smoke-launch only confirms "window opens and runs without error", not that the
> machine boots to its expected screen. A framebuffer/screenshot sweep over all
> of list A + B is owed once the native shells (list C) are tied in. See the
> `feedback_smoke_launch_not_boot` memory note.

## The harness today (`crates/emu198x-ui/src/lib.rs`)

`UiSystem` trait a runner implements, then calls `emu198x_ui::run(system,
runtime, scale, video)`. Capabilities present:

- Window + wgpu video (`raw`/`lcd`/`crt`), framed audio, gamepad, Esc-quit /
  F12-reset, halt overlay, display-aspect pixel-stretch.
- **Input:** console path (`button_map` + `map_key -> HostControl`) **and**
  keyboard path (`map_keys -> &[&str]` → `InputEvent::Key`). Both tracked for
  press/release + focus-loss.
- **Hooks:** `after_reset`, `handle_key`, `on_exit`, `halt_status`.
- **Missing (future capability work):** mouse/pointer, stateful key-mode
  toggles, native menu (#549), media UI (#550), save-states (#551), live
  variant switching (#554).

## The proven per-system recipe (one PR each)

For a **keyboard-only** headless system (e.g. ZX80 was a near-exact copy of
ZX81 — see `crates/emu198x-sinclair-zx81/src/ui.rs` as the template):

1. `Cargo.toml`: add `[features] default=["ui"]`, `ui=["dep:emu198x-ui"]`, and
   `emu198x-ui = { path="../emu198x-ui", version="0.2.0", optional=true }`.
2. New `src/ui.rs`: a unit `System` impl of `UiSystem` (window title, scale,
   `framebuffer_size` from `runtime.machine()`, `frame_ticks`/`frame_duration`
   from the system's clock, an empty `button_map`, `map_keys` from the
   machine's key-name scheme — find it in `runtime-*/src/input.rs`
   `key_from_name`), plus `Cli`/`parse_cli`/`run` (resolve ROM/firmware like the
   crate's `script.rs`).
3. `main.rs`: add `#[cfg(feature="ui")] mod ui;`, an `is_script_flag` set
   (`--script`/`--frames`/`--screenshot`/`--audio-capture`/…), default to `Ui`,
   and a `run_ui` with the `#[cfg(not(feature="ui"))]` fallback.
4. Verify: `cargo build -p <crate> --features ui` and `--no-default-features`;
   `cargo test -p <crate> --features ui`; `cargo clippy --all-targets --features
   ui -- -D warnings`; smoke-launch the window if a ROM is staged.
5. Commit `feat(<sys>): add … first native UI (#460)`, push, PR, then
   `gh pr merge <branch> --auto --merge --delete-branch`. `main` requires the 5
   CI checks (Format, Clippy, Build macos+windows, Coverage; set 2026-06-22), so
   `--auto` queues and lands only when green. Don't make a path-skipping check
   required — it would deadlock every merge.

## Ordered remaining work

**A. Keyboard-only headless systems — ✅ DONE (#600–#607, all merged).**
Jupiter Ace, Mattel Aquarius, Oric-Atmos, Memotech MTX, Tatung Einstein,
Acorn Atom, Sord M5, Spectravideo SVI-328. The whole category is on the harness.

**B. Joystick-led / console-like headless systems — ✅ DONE (#609–#618, merged).**
Sega Master System, Sega SG-1000, ColecoVision, Atari 5200, Atari 7800, MSX,
Commodore PET / VIC-20, Acorn BBC Micro / Electron. The whole category is on the
harness. Every headless runner now has a first UI; what remains (C/D) needs the
harness itself to grow.

**C. Harness capability lifts (substantial, lift from Spectrum `ui/`) — IN PROGRESS:**
- **save-states (#551) — ✅ DONE.** Quick-slot in the harness: `Cmd/Ctrl+S`
  quick-save, `Cmd/Ctrl+L` quick-load, one file per machine
  (`<profile_id>.state`) under `$EMU198X_STATE_DIR` or `~/.emu198x/state`. Leans
  entirely on the existing `MachineCore::snapshot()`/`restore()` (already in 30
  runtimes) + `profile()`, so **every** system got it with no per-system code.
  Gated on a host modifier so it never shadows the machines' bare S/L or
  F1-F10 keys. The menu-driven multi-slot / `.emu198xstate`-with-header file
  dialog (per `native-menu-shell.md` § State) is the later, menu-dependent cut.
- **native menu (#549) — ✅ FIRST CUT DONE.** `crates/emu198x-ui/src/menu.rs`
  lifts the Spectrum `ui/menu.rs` muda pattern into the harness, generalised:
  one `AppCommand` channel that **both** the menu and the keyboard shortcuts
  feed (so they never drift — the decision doc's core invariant). Menus: **App**
  (About/Quit, native `PredefinedMenuItem`), **Machine → Reset** (same
  `AppCommand::Reset` as F12), **State → Save/Load** (same as the Cmd/Ctrl+S/L
  chords), **View** (Window Scale 1–4× + Video Filter radios). muda is a
  non-Linux target dep with a Linux no-op stub (mirrors Spectrum's gating);
  `init_for_nsapp` runs once in `resumed`; menu events drained in
  `about_to_wait` → channel → `handle_command` at the frame boundary. **Still
  deferred** at that point: File → Open media (now done, below), Machine →
  variant switching. Windows menu attachment (`init_for_hwnd`) is a TODO; Linux
  has no bar (shortcuts cover everything).
- **media UI (#550) — ✅ DONE.** The File menu's **Open …** items are built
  generically from each machine's `profile().media_slots` — one item per slot,
  filtered by the slot's `MediaKind` — so a tapeless console gets no File menu
  and a disk machine gets the right slots, with **no per-system code**. A click
  emits `AppCommand::OpenMedia { slot, kind }`; the handler opens an `rfd` file
  picker, reads the image via `read_media_asset` (zip-aware), and inserts it
  with the generic `MachineCore::load_media`. `rfd` builds cross-platform
  (xdg-portal on Linux, no GTK) so it isn't target-gated, though the items are
  only reachable where the menu exists (macOS today). Some cartridge machines
  need a follow-up Machine → Reset to pick the new image up.
- Still to do, **needs design input**: mouse/pointer + stateful key-mode toggle
  (#552, mostly an Amiga/C64 need); live variant switching (#554, `LiveRuntime`
  trait → the Machine menu's variant radio).

**D. The remaining bespoke migrations (need the C capabilities):**
Amiga (#557 — mouse, dual joystick ports, keyboard-joystick toggle, 5 models,
DF0 ADF), C64 (#558 — tape/disk/dual-input/SID/multi-ROM), Dragon (#559 — axes,
semantic keymap, tape autoload), then **Spectrum (#560, last)** — re-point it at
the now-complete harness and delete its `ui/` module.

## Reference points

- Templates: `crates/emu198x-atari-2600/src/ui.rs` (console),
  `crates/emu198x-sinclair-zx81/src/ui.rs` (keyboard computer),
  `crates/emu198x-nes/src/ui.rs` (console + APU shortcuts + battery).
- Richest source to lift from: `crates/emu198x-spectrum/src/ui/`
  (`app.rs`/`input.rs`/`menu.rs`/`runner.rs`).
- Tracking issue #561; new-UI issue #460; capability issues #549–554.
