//! The Emu198x application icon on every desktop window.
//!
//! The PNGs are rendered from the SVGs in `crates/emu198x-app-icon/art/`
//! (see the README there). Each platform takes the icon differently:
//!
//! - **Windows** — winit's window icon is `ICON_SMALL` (title bar, 16-24px),
//!   so it gets the small drawing; the taskbar icon is `ICON_BIG` and gets the
//!   full plate. The `.exe` file icon Explorer shows is a separate resource,
//!   embedded at build time by `emu198x-app-icon`.
//! - **Linux** — one window icon (X11 `_NET_WM_ICON`); Wayland has no
//!   client-set icon, so the compositor looks for a `.desktop` entry instead.
//! - **macOS** — winit ignores window icons. Without an `.app` bundle the
//!   Dock shows a generic executable icon, so [`show_in_dock`] draws the
//!   plate into the Dock tile while the emulator runs.
//!
//! An icon that fails to load is reported and skipped: it never stops the
//! emulator starting.

use winit::window::{Icon, WindowAttributes};

/// The full plate, 256px: Linux, and the Windows taskbar.
const FULL_PNG: &[u8] = include_bytes!("../icon/emu198x-256.png");

/// The small drawing (the 'x' alone), 32px: the Windows title bar.
#[cfg(target_os = "windows")]
const SMALL_PNG: &[u8] = include_bytes!("../icon/emu198x-small-32.png");

/// The plate on Apple's icon grid, 512px: the macOS Dock tile.
#[cfg(target_os = "macos")]
const DOCK_PNG: &[u8] = include_bytes!("../icon/emu198x-macos-512.png");

/// A decoded image as straight RGBA8, the layout [`Icon::from_rgba`] takes.
#[derive(Debug)]
struct Rgba {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

/// Decode a PNG of any colour type to RGBA8.
fn decode_rgba(bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|err| err.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("icon PNG is too large to decode")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|err| err.to_string())?;
    buf.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 0xff])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[v, a]| [v, v, v, a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 0xff]).collect(),
        png::ColorType::Indexed => {
            return Err("icon PNG is still indexed after expansion".to_owned());
        }
    };
    Ok(Rgba {
        pixels,
        width: info.width,
        height: info.height,
    })
}

/// A winit icon from embedded PNG bytes, or `None` after reporting why not.
fn icon_from_png(bytes: &[u8]) -> Option<Icon> {
    let image = match decode_rgba(bytes) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("warning: could not use the application icon: {err}");
            return None;
        }
    };
    match Icon::from_rgba(image.pixels, image.width, image.height) {
        Ok(icon) => Some(icon),
        Err(err) => {
            eprintln!("warning: could not use the application icon: {err}");
            None
        }
    }
}

/// Add the application icon to a window's attributes.
pub(crate) fn with_icon(attributes: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes
            .with_window_icon(icon_from_png(SMALL_PNG))
            .with_taskbar_icon(icon_from_png(FULL_PNG))
    }
    #[cfg(not(target_os = "windows"))]
    {
        attributes.with_window_icon(icon_from_png(FULL_PNG))
    }
}

/// Draw the application icon into the macOS Dock tile. Call on the main
/// thread once the event loop is running (winit has created `NSApp` by then).
///
/// The Dock tile's content view rather than `NSApplication`'s
/// `applicationIconImage`: the setter for the latter is `unsafe` in
/// objc2-app-kit, and the workspace forbids `unsafe_code`. The tile covers the
/// Dock, which is where a bundle-less binary otherwise shows a generic icon.
#[cfg(target_os = "macos")]
pub(crate) fn show_in_dock() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage, NSImageScaling, NSImageView};
    use objc2_foundation::{NSData, NSPoint, NSRect};

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("warning: could not draw the Dock icon off the main thread");
        return;
    };
    let data = NSData::with_bytes(DOCK_PNG);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("warning: could not draw the Dock icon: AppKit could not read the PNG");
        return;
    };
    let tile = NSApplication::sharedApplication(mtm).dockTile();
    let view = NSImageView::imageViewWithImage(&image, mtm);
    view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), tile.size()));
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    tile.setContentView(Some(&view));
    tile.display();
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_dock() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_square_rgba(bytes: &[u8], side: u32) {
        let image = decode_rgba(bytes).expect("embedded icon decodes");
        assert_eq!((image.width, image.height), (side, side));
        assert_eq!(image.pixels.len(), (side * side * 4) as usize);
        assert!(Icon::from_rgba(image.pixels, side, side).is_ok());
    }

    #[test]
    fn embedded_icons_decode_to_rgba() {
        assert_square_rgba(FULL_PNG, 256);
        assert_square_rgba(include_bytes!("../icon/emu198x-small-32.png"), 32);
        assert_square_rgba(include_bytes!("../icon/emu198x-macos-512.png"), 512);
    }

    #[test]
    fn full_plate_corners_are_transparent_and_centre_opaque() {
        let image = decode_rgba(FULL_PNG).expect("embedded icon decodes");
        let alpha = |x: u32, y: u32| image.pixels[((y * image.width + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "rounded corner is transparent");
        assert_eq!(alpha(128, 128), 0xff, "plate is opaque");
    }

    #[test]
    fn a_bad_png_is_skipped_not_fatal() {
        assert!(icon_from_png(b"not a png").is_none());
    }
}
