//! `emu198x-amiga` — Commodore Amiga native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`), its
//! headless runner (`src/script.rs`), its MCP debugging tools
//! (`src/mcp/`), and its window (`src/ui.rs`). Model selection and
//! Kickstart resolution are the runtime crate's catalogue, resolved by
//! the shell. Building with `--no-default-features` drops the `ui`
//! feature (winit + wgpu) for the MCP debugging surface and the headless
//! capture pipeline.
//!
//! `src/mcp/{tools,lvo}.rs` are also `#[path]`-included by the
//! `mcp_smoke` integration test (a second crate root), so they must not
//! depend on the rest of the binary.

mod app;
mod mcp;
mod script;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Amiga>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Amiga>();
}
