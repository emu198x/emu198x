//! The one unsafe call: handing a winit window's HWND to `muda`.

use std::fmt;

use muda::Menu;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Why the menu could not be attached.
#[derive(Debug)]
pub enum AttachError {
    /// winit could not give a handle for the window.
    NoHandle(String),
    /// The window is not a Win32 window.
    NotWin32,
    /// `muda` refused the window.
    Menu(muda::Error),
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHandle(err) => write!(f, "the window has no handle: {err}"),
            Self::NotWin32 => f.write_str("the window is not a Win32 window"),
            Self::Menu(err) => write!(f, "the menu could not be attached: {err}"),
        }
    }
}

impl std::error::Error for AttachError {}

/// Attaches `menu` as the menu bar of `window`.
///
/// Call it on the thread that runs the event loop, which is the thread that
/// created the window.
///
/// # Errors
///
/// Returns an error if the window has no Win32 handle or `muda` rejects it.
pub fn attach(menu: &Menu, window: &Window) -> Result<(), AttachError> {
    let handle = window
        .window_handle()
        .map_err(|err| AttachError::NoHandle(err.to_string()))?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return Err(AttachError::NotWin32);
    };
    // SAFETY: `init_for_hwnd` needs a valid HWND. This one comes from
    // `window`, which is alive for the whole call because we borrow it, and
    // winit hands out a window handle only while the window exists. winit
    // creates its windows on the event-loop thread, which is the thread this
    // function's contract asks callers to use, as `muda` requires.
    unsafe { menu.init_for_hwnd(win32.hwnd.get()) }.map_err(AttachError::Menu)
}
