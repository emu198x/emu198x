//! Shared script and stdio MCP entry point for the Apple II family.
mod app;
fn main() {
    emu198x_shell::launch::main_headless::<app::AppleII>();
}
