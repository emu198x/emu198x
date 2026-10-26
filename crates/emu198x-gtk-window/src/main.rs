//! `emu198x-gtk-window`: the GTK4 window the Linux machine binaries open.
//!
//! A machine binary starts this program and keeps the machine itself. Frames
//! and window requests arrive on standard input; input and menu commands go
//! back on standard output, in the messages of
//! [`emu198x_ui::window_protocol`]. Keeping GTK in its own process means the
//! machine binaries never link it, so their headless, script and MCP modes
//! run where GTK is not installed. See
//! `knowledge/decisions/gtk-window-process.md`.
//!
//! Standard output carries the protocol, so nothing here prints to it;
//! diagnostics go to standard error.

#[cfg(all(target_os = "linux", feature = "gtk"))]
mod keys;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod menu;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod outbox;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod picture;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod screen;
#[cfg(all(target_os = "linux", feature = "gtk"))]
mod window;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(all(target_os = "linux", feature = "gtk"))]
    {
        window::main()
    }
    #[cfg(not(all(target_os = "linux", feature = "gtk")))]
    {
        eprintln!("emu198x-gtk-window: the GTK window is built for Linux only");
        ExitCode::FAILURE
    }
}
