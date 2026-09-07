//! `emu198x-amiga` — Commodore Amiga native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`), model
//! selection and Kickstart resolution (`src/model.rs`), its headless
//! runner (`src/script.rs`), its MCP debugging tools (`src/mcp/`), and
//! its window (`src/ui.rs`). Building with `--no-default-features` drops
//! the `ui` feature (winit + wgpu) for the MCP debugging surface and the
//! headless capture pipeline.
//!
//! `model.rs` and `src/mcp/{tools,lvo}.rs` are also `#[path]`-included by
//! the `mcp_smoke` integration test (a second crate root), so they refer
//! to each other as `crate::model::…` and must not depend on the rest of
//! the binary.

mod app;
mod mcp;
mod model;
mod script;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Amiga>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Amiga>();
}
