//! `emu198x-c64` — Commodore 64 native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`), its
//! headless runner (`src/script.rs`), its MCP tools (`src/mcp_tools.rs`),
//! and its window (`src/ui.rs`). Building with `--no-default-features`
//! drops the `ui` feature (winit + wgpu + muda) for the headless
//! boot/capture/trace pipeline.

mod app;
mod mcp_tools;
mod script;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::C64>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::C64>();
}
