//! `emu198x-atari-5200` — Atari 5200 native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`) and its
//! window (`src/ui.rs`). Building with `--no-default-features` drops the
//! `ui` feature (winit + wgpu) for the headless screenshot / script pipeline.

mod app;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Atari5200>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Atari5200>();
}
