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

(Plus the unrelated Atari 2600 Supercharger work: #588, #593.)

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
5. Commit `feat(<sys>): add … first native UI (#460)`, push, PR, **let CI
   register (~30 s) then `gh pr checks --watch` then `gh pr merge --merge
   --delete-branch`**. NB: `--auto` and an immediate `--watch` both merge
   *before* CI here (no required checks) — sleep first so the watch really waits.

## Ordered remaining work

**A. Keyboard-only headless systems (mechanical, recipe above):**
Jupiter Ace, Mattel Aquarius, Oric-Atmos, Memotech MTX, Tatung Einstein,
Acorn Atom, Sord M5, Spectravideo SVI-328 — check each `runtime-*/src/input.rs`
for the key scheme; some add a joystick (use `button_map` + `map_key` too).

**B. Joystick-led / console-like headless systems (existing console path):**
Sega Master System, Sega SG-1000, ColecoVision, Atari 5200, Atari 7800, MSX
(keyboard + joystick), Commodore PET / VIC-20, Acorn BBC Micro / Electron.

**C. Harness capability lifts (substantial, lift from Spectrum `ui/`):**
mouse/pointer input + stateful key-mode toggle (#552 remainder), native menu
(#549, `menu.rs` + `AppCommand`, honour `knowledge/decisions/native-menu-shell.md`),
media UI (#550), save-states (#551), live variant switching (#554,
`LiveRuntime` trait).

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
