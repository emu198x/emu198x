//! `emu198x-spectrum` — Spectrum SOLID native binary.
//!
//! One binary, three modes: UI (default), headless script, and MCP. The
//! modes themselves belong to the shared launcher in `emu198x-shell` and
//! `emu198x-ui`; this crate supplies the machine (`src/app.rs`), its
//! window (`src/ui.rs`), and the two headless bodies the shared loop
//! cannot express: the script runner (`src/script/`), which picks the
//! boot variant from the script and intercepts the family's steps, and
//! the MCP server (`src/mcp/`) with the Spectrum tool set. Shared state:
//! `src/machine.rs` (MachineKind, ROM resolver).
//!
//! See `docs/brainstorms/2026-05-08-track-1b-single-binary-brainstorm.md`
//! for the design that drove this layout.
//!
//! # Cargo features
//!
//! - `ui` (default) — compiles in the shared `emu198x-ui` harness (winit +
//!   wgpu + muda) for the interactive window, native menu, and framed
//!   audio/video loop. Required for the default UI mode.
//! - Without `ui` — `--script` and `--mcp` modes still work; asking for a
//!   window errors at runtime with a "rebuild with `--features ui`"
//!   message. Code198x's headless screenshot/video pipeline uses this
//!   build to skip the heavy graphics stack.

mod app;
mod machine;
mod mcp;
mod portable_snapshot;
mod script;

#[cfg(feature = "ui")]
mod ui;

fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::Spectrum>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::Spectrum>();
}
