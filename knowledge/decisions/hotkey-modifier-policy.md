# Decision: Hotkey modifier policy

**Date:** 2026-04-09

## The decision

**Alt is the only modifier available for emulator hotkeys across the Spectrum family (and will be across every 8-bit system). Ctrl and Shift are permanently off-limits as hotkey modifiers.**

Cmd (macOS) is usable as a Mac-only alias when a native feel matters, but Alt is the canonical cross-platform binding and must always exist.

## Why

The Spectrum keyboard matrix (see `crates/emu-sinclair-zx-spectrum/src/main.rs` `update_keyboard`) is scanned from physical SDL scancodes every frame. Two modifier keys are load-bearing *as Spectrum keys*, not hotkey modifiers:

- **`LCtrl` / `RCtrl` → SYMBOL SHIFT.** SYMBOL SHIFT is how BASIC keywords are typed: SS+S = `NOT`, SS+P = `"`, SS+O = `;`, SS+J = `-`, SS+L = `=`. Stealing Ctrl for hotkeys breaks BASIC entry for every programmer.
- **`LShift` / `RShift` → CAPS SHIFT.** CAPS SHIFT is how 48K arrow keys, `DELETE`, `EDIT`, `BREAK`, and inverse video are typed. Stealing Shift breaks navigation on every game that polls it.

That leaves Alt and Cmd as the only *physically present modern keys* that are neither on the Spectrum matrix nor load-bearing for some other system integration:

- **Alt** — free on every platform, present on every keyboard, ignored by every window manager we care about.
- **Cmd (`LGui`/`RGui`)** — free on Mac, but Linux's Super and Windows' Win key are typically captured by the desktop environment before SDL sees the keydown.

Alt wins as the canonical modifier. Cmd can be added as a Mac-only *alias* for a hotkey that already has an Alt binding — never as the sole binding.

## Consequences

**`update_keyboard` must be modifier-aware.**

When Alt (or any adopted modifier) is held, the matrix scan must skip letter/digit injection for that frame. Otherwise pressing `Alt+S` simultaneously fires the hotkey *and* types `S` in BASIC. One guard at the top of `update_keyboard` handles the whole matrix:

```rust
if state.is_scancode_pressed(Scancode::LAlt) || state.is_scancode_pressed(Scancode::RAlt) {
    return;
}
```

The guard matches *any* modifier adopted under this policy — today that's Alt, tomorrow it may also include Cmd for the Mac-only alias path.

**F-keys are usable but discouraged as primary bindings.**

On macOS the default keyboard setting requires `Fn+F5` to get a real F5 keystroke (media keys own F1–F12 otherwise). Users can change the system setting, but defaults matter — any hotkey that is *only* reachable via an F-key is a hotkey most Mac users will never press. F5 is fine for tape-play (it's a one-off, discoverable feature, already bound pre-policy). New hotkeys should land on Alt-modified letters first.

**Printable punctuation is not free.**

`-`, `=`, `[`, `]`, `;`, `'`, `,`, `.`, `/` are currently unmapped in `update_keyboard` but represent Spectrum keys via SYMBOL SHIFT combos. Binding them to hotkeys commits to never passing them through to the Spectrum matrix, even when Phase 2+ improves punctuation mapping. Only use them if Alt coverage is somehow impossible.

## Currently assigned hotkeys

| Hotkey | Action | Notes |
|--------|--------|-------|
| `Escape` | Quit | Pre-policy, uniquely safe (never on a Spectrum) |
| `F5` | Tape-play | Pre-policy, kept for continuity |
| `Alt+S` | Quick-save state | Phase 1.1 |
| `Alt+L` | Quick-load state | Phase 1.1 |

## Drift triggers

Hotkey modifier drift usually comes dressed as "following desktop convention." If I'm about to propose any of these, stop and re-read this entry.

**Code patterns to reject:**

- `Scancode::LCtrl` in a `KeyDown` match arm as a hotkey modifier
- `Scancode::LShift` in a `KeyDown` match arm as a hotkey modifier
- `KMOD_CTRL` / `KMOD_SHIFT` as the only modifier check for a hotkey
- Removing the Alt guard in `update_keyboard` as "cleanup"
- New punctuation bindings (`Scancode::Minus`, `Scancode::LeftBracket`, …) without a comment explaining why Alt isn't an option

**Phrases that signal drift:**

- "Cmd+S on Mac, Ctrl+S on Linux" — Ctrl is SYMBOL SHIFT, this breaks BASIC
- "Shift+F1 for load slot 1" — Shift is CAPS SHIFT, this breaks 48K arrow keys
- "Everyone knows Ctrl+S means save" — everyone who isn't holding SYMBOL SHIFT, yes
- "The matrix scan is fine, users won't notice the modifier collision" — they will, the first time BASIC rejects `NOT` as a syntax error
- "Let's just bind the hotkey to a single punctuation key, no modifier needed" — check `update_keyboard` and the ROM's SYMBOL SHIFT table first

**What to do when triggered:** use Alt. If Alt is already taken for the function the user is trying to bind, add a different modifier *combination* on Alt (`Alt+Shift+X`? No — Shift is CAPS SHIFT. `Alt+Ctrl+X`? No — Ctrl is SYMBOL SHIFT). The answer is that Alt+letter pairs are the only space, so be deliberate about allocation. Keep a running list in this file as hotkeys are added.

## Related

- [Save state format](save-state-format.md) — the first consumer of this policy (Alt+S, Alt+L for quick-save/load)
- [Product roadmap](product-roadmap.md) — future systems (C64, Amiga, NES) will have their own keyboard/joystick layouts but the Alt-only rule extends to all of them to keep hotkeys consistent across the launcher
