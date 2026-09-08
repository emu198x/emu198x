//! `emu198x-nes` — Nintendo NES native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`, with the
//! Blargg assertion and the smoke sweep riding on the shared loop), its
//! MCP dump tools (`src/mcp_tools.rs`), and its window (`src/ui.rs`).
//! Building with `--no-default-features` drops the `ui` feature (winit +
//! wgpu) for the headless screenshot / smoke / Blargg pipeline.

mod app;
mod mcp_tools;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Nes>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Nes>();
}
