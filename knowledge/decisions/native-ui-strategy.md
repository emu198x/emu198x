# Decision: Native UI Strategy

**Date:** April 2026

## The decision

Platform-specific native frontends for each OS. SwiftUI on macOS, GTK4 on Linux, WinUI on Windows. Each calls the same Rust emulation library via FFI. The Rust core is a library, not an application.

## For October (cross-platform baseline)

Cross-platform windowing layer (SDL2 or winit) with native platform menus (NSMenu on macOS, GTK menu on Linux, Win32 menu on Windows) and native file dialogs via thin FFI. Not a full native app — the chrome is real but minimal. This ships for all platforms and is the foundation the capture pipeline runs on.

**Implementation locked 2026-05-06: winit + `muda` + `rfd`.** winit handles windowing/input/GPU surface; `muda` provides native menus by calling NSMenu / GTK4 menu / Win32 menu directly; `rfd` provides native file dialogs by calling NSOpenPanel / GtkFileChooser / IFileOpenDialog. All three are thin wrappers around platform APIs — they call the OS's own widgets rather than rendering their own. The original "SDL2 windowed runner" wording named the library type (cross-platform windowing layer), not the specific library; winit fills that role equivalently and is what the codebase already uses.

## Post-launch (true native frontends)

- **macOS**: SwiftUI app wrapping the Rust core. Native menus, toolbar, preferences, split views for debugger. Swift 6 strict concurrency.
- **Linux**: GTK4 via gtk4-rs. Native look on GNOME/KDE.
- **Windows**: WinUI or equivalent. Native look on Windows 11+.

Each frontend calls the same Rust library. The system trait boundary (`run_frame()` → framebuffer + audio) is the FFI surface.

## Why not egui/iced/Tauri

Previous attempt used egui. The result felt like a game engine UI pretending to be an app. Strong feedback: "we fucked this up before by using crap." Native UI is non-negotiable for a product that ships to users.

Tauri (webview) looks polished but isn't native widgets. iced is retained-mode Rust but still not platform-native controls. None of these are acceptable long-term.

## Drift triggers

Non-native UI is the drift that already burned this project once. If I'm about to propose any of these, stop and re-read the "Why not egui/iced/Tauri" section above.

**Dependencies permitted (added 2026-05-06):**

These are thin wrappers around platform APIs — they call the OS's own widgets via FFI, they don't render their own:

- `winit` — cross-platform windowing/input/GPU surface (the layer the menus and dialogs sit on top of)
- `muda` — native menu bar / context menus, wraps NSMenu / GTK4 menu / Win32 menu
- `rfd` — native file dialogs, wraps NSOpenPanel / GtkFileChooser / IFileOpenDialog
- `tao` — winit fork with menus built in; functionally similar to winit + muda; permitted as a substitute if the consolidation is preferable

**Dependencies to reject:**

- `egui`, `eframe` — looks like a game engine pretending to be an app
- `iced` — retained-mode Rust but not platform-native widgets
- `tauri`, `wry`, any webview-based framework — not native
- `dioxus`, `leptos`, or any web-rendering frontend for the shell
- Cross-platform GUI abstractions that render their own widgets (GTK bindings that "also work on macOS", etc.) — distinguished from thin native-API wrappers like `muda` and `rfd`, which are permitted (see above). The line: permitted = calls platform widgets via FFI; rejected = renders its own widgets cross-platform.

**Phrases that signal drift:**

- "Let's use egui for the baseline, we can swap it out later"
- "Tauri would let us reuse the Code198x web UI"
- "A webview is close enough to native for now"
- "Iced is Rust-native, that counts"
- "We can prototype in [non-native framework] and port to native later"
- "Cross-platform GUI framework" in any framing
- "We'll swap it out post-launch"

**What the user has said about this directly:** *"we fucked this up before by using crap."* Native UI is non-negotiable. The October SDL2 baseline is the *only* non-native frontend allowed, and that's the headless-runner foundation with native platform menus bolted on — not the product UI. If I'm proposing egui/iced/Tauri as a shortcut, I'm proposing to repeat the exact mistake this entry was written to prevent.

## Related

- [Product roadmap](product-roadmap.md) — October timeline and feature priorities
- [October catalogue](october-catalogue.md) — Spectrum SOLID criterion 7 (native UI minimum) anchors here

## Log

| Date | Event |
|---|---|
| 2026-04 | Decision created. Per-platform native frontends (SwiftUI/GTK4/WinUI) for long-term; SDL2 + native menus baseline for October. |
| 2026-05-06 | **Amended.** Implementation locked as winit + `muda` + `rfd` for the October baseline. Permitted-dependencies section added to distinguish thin native-API wrappers (acceptable) from cross-platform widget renderers (rejected). The original "SDL2" mention reframed as "cross-platform windowing layer (SDL2 or winit)" — winit fills the same role and is what the codebase uses. |
