//! The picture: each frame composed at the widget's device-pixel size.
//!
//! The same wgpu presenter the winit window uses renders every filter, raw
//! included, offscreen, so this window shows exactly what the winit window
//! does. Without a GPU adapter the raw frame is scaled by nearest neighbour
//! on the CPU instead.

use emu198x_native_video::{PresentationProfile, WgpuVideoPresenter};
use emu198x_shell::CapturedFrame;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::screen::Screen;

/// Scale an RGBA frame by nearest neighbour into `rect` of a black
/// `width` × `height` picture.
pub(crate) fn nearest_into(
    pixels: &[u8],
    frame_w: u32,
    frame_h: u32,
    width: u32,
    height: u32,
    rect: &gtk4::graphene::Rect,
) -> Vec<u8> {
    let mut out = vec![0u8; width as usize * height as usize * 4];
    for pixel in out.as_chunks_mut::<4>().0 {
        pixel[3] = 0xFF;
    }
    let (x0, y0) = (rect.x().max(0.0) as u32, rect.y().max(0.0) as u32);
    let (rw, rh) = (rect.width().max(1.0), rect.height().max(1.0));
    let x1 = (rect.x() + rect.width()).min(width as f32).max(0.0) as u32;
    let y1 = (rect.y() + rect.height()).min(height as f32).max(0.0) as u32;
    for y in y0..y1 {
        let sy = (((y - y0) as f32 + 0.5) * frame_h as f32 / rh) as u32;
        let sy = sy.min(frame_h.saturating_sub(1)) as usize;
        for x in x0..x1 {
            let sx = (((x - x0) as f32 + 0.5) * frame_w as f32 / rw) as u32;
            let sx = sx.min(frame_w.saturating_sub(1)) as usize;
            let src = (sy * frame_w as usize + sx) * 4;
            let dst = (y as usize * width as usize + x as usize) * 4;
            if let (Some(from), Some(to)) = (pixels.get(src..src + 4), out.get_mut(dst..dst + 4)) {
                to.copy_from_slice(from);
            }
        }
    }
    out
}

/// The wgpu presenter, rendering offscreen at the picture widget's size,
/// made the first time a frame is shown.
#[derive(Default)]
pub(crate) struct Filters {
    pub(crate) presenter: Option<WgpuVideoPresenter>,
    /// Set once the presenter could not be made, so it is not tried on every
    /// frame. The host then shows the raw picture.
    unavailable: bool,
    /// The last error a frame hit, reported once until it changes.
    last_error: Option<String>,
}

impl Filters {
    /// Compose `frame` for the whole widget, or `None` to fall back to raw.
    pub(crate) fn compose(
        &mut self,
        screen: &Screen,
        frame: &CapturedFrame,
        profile: &PresentationProfile,
    ) -> Option<gdk::Texture> {
        if self.unavailable {
            return None;
        }
        let (width, height) = screen.device_size();
        if self.presenter.is_none() {
            match WgpuVideoPresenter::new_offscreen(width, height, frame.width, frame.height) {
                Ok(presenter) => self.presenter = Some(presenter),
                Err(err) => {
                    eprintln!("video: filters are unavailable, showing the raw picture: {err}");
                    self.unavailable = true;
                    return None;
                }
            }
        }
        let presenter = self.presenter.as_mut()?;
        if presenter.offscreen_pixels().map(|(_, w, h)| (w, h)) != Some((width, height)) {
            presenter.resize_surface(width, height);
        }
        match presenter.present_with_status(frame, profile) {
            Ok(true) => self.last_error = None,
            Ok(false) => return None,
            Err(err) => {
                let message = err.to_string();
                if self.last_error.as_deref() != Some(message.as_str()) {
                    eprintln!("video: {message}; showing the raw picture");
                    self.last_error = Some(message);
                }
                return None;
            }
        }
        let (pixels, width, height) = presenter.offscreen_pixels()?;
        let texture = gdk::MemoryTexture::new(
            i32::try_from(width).ok()?,
            i32::try_from(height).ok()?,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from(pixels),
            width as usize * 4,
        );
        Some(texture.upcast())
    }
}

#[cfg(test)]
mod tests {
    use super::nearest_into;
    use gtk4::graphene::Rect;

    #[test]
    fn the_fallback_scales_by_nearest_into_the_letterbox() {
        // A 2 × 1 frame, red then blue, into the middle 4 × 2 of a 6 × 2
        // picture: each source pixel becomes a 2 × 2 block, and the
        // letterbox columns stay opaque black.
        let red = [0xFF, 0, 0, 0xFF];
        let blue = [0, 0, 0xFF, 0xFF];
        let out = nearest_into(
            &[red, blue].concat(),
            2,
            1,
            6,
            2,
            &Rect::new(1.0, 0.0, 4.0, 2.0),
        );
        let at = |x: usize, y: usize| &out[(y * 6 + x) * 4..(y * 6 + x) * 4 + 4];
        for y in 0..2 {
            assert_eq!(at(0, y), &[0, 0, 0, 0xFF]);
            assert_eq!(at(1, y), &red);
            assert_eq!(at(2, y), &red);
            assert_eq!(at(3, y), &blue);
            assert_eq!(at(4, y), &blue);
            assert_eq!(at(5, y), &[0, 0, 0, 0xFF]);
        }
    }
}
