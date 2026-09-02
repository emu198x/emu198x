//! Frame collection for a canvas.
//!
//! Machines emit either `Rgba8888` or `Indexed8` frames. A canvas wants
//! RGBA bytes, so indexed frames need their palette applied.
//!
//! That conversion is **not** reimplemented here. `CapturedFrame` already
//! does it, correctly, and is what the native screenshot and video paths
//! use. Palette entries are `0xRRGGBBAA`, and a hand-rolled conversion that
//! assumes `0xAARRGGBB` renders black as pure blue and normal white as
//! lavender — a picture that looks plausible enough to ship.

use emu198x_shell::{CaptureError, CapturedFrame, FramePacket, FrameSink, MachineError};

/// The most recent frame, as RGBA bytes ready for a canvas.
#[derive(Debug, Default)]
pub struct RgbaFrame {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    /// Set when a frame arrived but could not be converted, so the caller
    /// can tell "no frame yet" from "the frame was rejected".
    last_error: Option<CaptureError>,
}

impl RgbaFrame {
    /// Creates an empty frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// RGBA bytes of the last converted frame, empty before the first one.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Width and height in pixels of the last converted frame.
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The error from the most recent frame that failed to convert.
    #[must_use]
    pub const fn last_error(&self) -> Option<&CaptureError> {
        self.last_error.as_ref()
    }
}

impl FrameSink for RgbaFrame {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        let width = frame.width;
        let height = frame.height;
        let captured = CapturedFrame::from_packet(frame);

        match captured.rgba_pixels() {
            Ok(pixels) => {
                self.pixels = pixels;
                self.width = width;
                self.height = height;
                self.last_error = None;
            }
            Err(error) => {
                // Keep the previous frame rather than blanking the canvas: a
                // single malformed packet should not clear the picture.
                self.last_error = Some(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::{MachineTime, PixelFormat};

    fn packet<'a>(
        format: PixelFormat,
        pixels: &'a [u8],
        palette: Option<&'a [u32]>,
        width: u32,
        height: u32,
    ) -> FramePacket<'a> {
        FramePacket {
            timestamp: MachineTime::new(0),
            format,
            width,
            height,
            palette,
            pixels,
        }
    }

    #[test]
    fn an_indexed_frame_applies_its_palette_in_rgba_order() {
        // 0xRRGGBBAA. Getting this backwards is the #1413-shaped mistake:
        // black would come out pure blue.
        let palette = [0x00_00_00_FF_u32, 0xFF_FF_FF_FF_u32];
        let mut frame = RgbaFrame::new();

        frame
            .push_frame(packet(PixelFormat::Indexed8, &[0, 1], Some(&palette), 2, 1))
            .expect("sink accepts the frame");

        assert_eq!(
            frame.pixels(),
            &[0, 0, 0, 255, 255, 255, 255, 255],
            "black must stay black and white stay white"
        );
        assert_eq!(frame.size(), (2, 1));
    }

    #[test]
    fn an_rgba_frame_passes_through_unchanged() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut frame = RgbaFrame::new();

        frame
            .push_frame(packet(PixelFormat::Rgba8888, &pixels, None, 2, 1))
            .expect("sink accepts the frame");

        assert_eq!(frame.pixels(), &pixels);
    }

    #[test]
    fn a_malformed_frame_keeps_the_previous_picture() {
        let palette = [0xFF_FF_FF_FF_u32];
        let mut frame = RgbaFrame::new();
        frame
            .push_frame(packet(PixelFormat::Indexed8, &[0], Some(&palette), 1, 1))
            .expect("first frame");
        let good = frame.pixels().to_vec();

        // Indexed frame whose pixel count does not match its dimensions.
        frame
            .push_frame(packet(PixelFormat::Indexed8, &[0], Some(&palette), 64, 64))
            .expect("a bad frame is reported, not propagated as a machine error");

        assert_eq!(frame.pixels(), good, "the canvas kept the last good frame");
        assert!(frame.last_error().is_some(), "the failure was recorded");
    }
}
