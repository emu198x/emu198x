//! Shared native window, script and stdio MCP entry point for the Apple II.
mod app;
#[cfg(feature = "ui")]
mod ui;
fn main() {
    #[cfg(feature = "ui")]
    emu198x_ui::launch::main::<app::AppleII>();
    #[cfg(not(feature = "ui"))]
    emu198x_shell::launch::main_headless::<app::AppleII>();
}
