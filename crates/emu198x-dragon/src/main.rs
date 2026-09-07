//! `emu198x-dragon` — Dragon 32/64 native binary.
//!
//! One binary, three modes: UI (default), headless harness / script, and
//! MCP. The modes themselves belong to the shared launcher in
//! `emu198x-shell` and `emu198x-ui`; this crate supplies the machine
//! (`src/app.rs`), the bring-up harness the headless mode runs
//! (`src/script.rs`: smoke matrices, typed commands, trace watches, XRoar
//! references), and its window (`src/ui.rs`). Building with
//! `--no-default-features` drops the `ui` feature (winit + wgpu) for the
//! headless smoke / trace / XRoar pipeline.

mod app;
mod script;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Dragon>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Dragon>();
}
