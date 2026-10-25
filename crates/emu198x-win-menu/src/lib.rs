//! Attaches the shared native menu bar to a window on Windows.
//!
//! `muda` attaches a menu to a Win32 window through `Menu::init_for_hwnd`,
//! which is an unsafe function because it trusts the caller's window
//! handle. The workspace forbids unsafe code, so that one call lives here
//! rather than in `emu198x-ui`, which keeps the workspace's `forbid`. Only
//! the `attach` module may use unsafe code. See
//! `knowledge/decisions/windows-menu-unsafe-exception.md`.
//!
//! On every other platform this crate is empty.

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod attach;

#[cfg(target_os = "windows")]
pub use attach::{AttachError, attach};
