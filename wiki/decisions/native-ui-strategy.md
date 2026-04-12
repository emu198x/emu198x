# Decision: Native UI Strategy

**Date:** April 2026

## The decision

Platform-specific native frontends for each OS. SwiftUI on macOS, GTK4 on Linux, WinUI on Windows. Each calls the same Rust emulation library via FFI. The Rust core is a library, not an application.

## For October (cross-platform baseline)

SDL2 windowed runner with native platform menus (NSMenu on macOS, etc.) via thin FFI. Not a full native app — the chrome is real but minimal. This ships for all platforms and is the foundation the capture pipeline runs on.

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

**Dependencies to reject:**

- `egui`, `eframe` — looks like a game engine pretending to be an app
- `iced` — retained-mode Rust but not platform-native widgets
- `tauri`, `wry`, any webview-based framework — not native
- `dioxus`, `leptos`, or any web-rendering frontend for the shell
- Cross-platform GUI abstractions (GTK bindings that "also work on macOS", etc.)

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
