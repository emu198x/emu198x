//! Host-side sinks and packets shared by frontends, capture, and automation.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::error::MachineError;
use crate::time::MachineTime;

/// Pixel layout for one frame packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PixelFormat {
    /// One byte per pixel plus a palette.
    Indexed8,
    /// Four bytes per pixel in `R`, `G`, `B`, `A` order.
    Rgba8888,
}

/// One frame of raw video output emitted by a machine.
#[derive(Debug)]
pub struct FramePacket<'a> {
    /// Machine timestamp at which the frame became available.
    pub timestamp: MachineTime,
    /// Pixel layout of `pixels`.
    pub format: PixelFormat,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Palette data for indexed formats.
    pub palette: Option<&'a [u32]>,
    /// Raw pixel bytes.
    pub pixels: &'a [u8],
}

/// One audio packet emitted by a machine.
#[derive(Debug)]
pub struct AudioPacket<'a> {
    /// Machine timestamp at which the packet became available.
    pub timestamp: MachineTime,
    /// Output sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels in `samples`.
    pub channels: u8,
    /// Interleaved samples.
    pub samples: &'a [f32],
}

/// One machine-local trace event emitted to host tooling.
#[derive(Debug)]
pub struct TraceEvent<'a> {
    /// Machine timestamp associated with the event.
    pub timestamp: MachineTime,
    /// Stable event kind identifier.
    pub kind: Cow<'static, str>,
    /// Event payload encoded by the emitting machine.
    pub payload: &'a [u8],
}

/// Generic host input event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InputEvent {
    /// A named key changed state.
    Key {
        /// Stable symbolic key name.
        name: Cow<'static, str>,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
    /// A named control on a numbered port changed state.
    Button {
        /// Target port number.
        port: u8,
        /// Stable symbolic control name.
        name: Cow<'static, str>,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
    /// A named analogue axis on a numbered port changed value.
    Axis {
        /// Target port number.
        port: u8,
        /// Stable symbolic axis name.
        name: Cow<'static, str>,
        /// Normalized signed axis value, from full negative to full positive.
        value: i16,
    },
    /// Relative pointer movement.
    PointerMotion {
        /// Stable device name, for example `"mouse-1"`.
        device: Cow<'static, str>,
        /// Horizontal movement delta.
        dx: i32,
        /// Vertical movement delta.
        dy: i32,
    },
    /// A pointing device is aimed at a position on the emulated display.
    ///
    /// Absolute, where [`PointerMotion`](Self::PointerMotion) is relative: a
    /// light gun, light pen or graphics tablet reports *where it is*, not how
    /// far it moved. The two are not interchangeable — a relative stream
    /// cannot answer "where is it now" without state the host does not have,
    /// and an absolute one cannot express a mouse being moved past the edge
    /// of the screen.
    ///
    /// `at` is normalized across the framebuffer, full negative at the left or
    /// top edge and full positive at the right or bottom, so it survives a
    /// machine changing its display size mid-run. Use
    /// [`aim_to_pixels`] and [`aim_from_pixels`] to convert.
    ///
    /// `None` means the device is pointed off the display altogether. That is
    /// a real gesture rather than an error: light-gun games routinely use
    /// "point away and pull the trigger" to reload, and it has to be
    /// distinguishable from a shot at the edge of the picture.
    Aim {
        /// Target port number.
        port: u8,
        /// Normalized position across the framebuffer, or `None` when the
        /// device is pointed off it.
        at: Option<(i16, i16)>,
    },
    /// Pointer button change.
    PointerButton {
        /// Stable device name, for example `"mouse-1"`.
        device: Cow<'static, str>,
        /// Stable button name.
        button: Cow<'static, str>,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
}

/// Receives raw frame output from a machine.
pub trait FrameSink {
    /// Accepts one frame packet.
    ///
    /// # Errors
    ///
    /// Returns an error if the sink rejects the packet.
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError>;
}

/// Receives raw audio output from a machine.
pub trait AudioSink {
    /// Accepts one audio packet.
    ///
    /// # Errors
    ///
    /// Returns an error if the sink rejects the packet.
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError>;
}

/// Receives machine-local trace data.
pub trait TraceSink {
    /// Accepts one trace event.
    ///
    /// # Errors
    ///
    /// Returns an error if the sink rejects the event.
    fn push_trace(&mut self, event: TraceEvent<'_>) -> Result<(), MachineError>;
}

/// No-op frame sink.
#[derive(Debug, Default)]
pub struct NullFrameSink;

impl FrameSink for NullFrameSink {
    fn push_frame(&mut self, _frame: FramePacket<'_>) -> Result<(), MachineError> {
        Ok(())
    }
}

/// No-op audio sink.
#[derive(Debug, Default)]
pub struct NullAudioSink;

impl AudioSink for NullAudioSink {
    fn push_audio(&mut self, _packet: AudioPacket<'_>) -> Result<(), MachineError> {
        Ok(())
    }
}

/// No-op trace sink.
#[derive(Debug, Default)]
pub struct NullTraceSink;

impl TraceSink for NullTraceSink {
    fn push_trace(&mut self, _event: TraceEvent<'_>) -> Result<(), MachineError> {
        Ok(())
    }
}

/// Host-side services visible to a machine during execution.
pub struct HostIo<'a> {
    /// Input events queued by the host before this execution slice.
    pub input_events: &'a [InputEvent],
    /// Raw frame sink.
    pub frame_sink: &'a mut dyn FrameSink,
    /// Raw audio sink.
    pub audio_sink: &'a mut dyn AudioSink,
    /// Raw trace sink.
    pub trace_sink: &'a mut dyn TraceSink,
}

/// Convert a normalized [`Aim`](InputEvent::Aim) to a pixel on a display
/// `width` by `height`.
///
/// The mapping spans the whole display, so full negative lands on the first
/// pixel and full positive on the last rather than one past either end.
#[must_use]
pub fn aim_to_pixels(at: (i16, i16), width: u32, height: u32) -> (u16, u16) {
    fn axis(value: i16, extent: u32) -> u16 {
        if extent == 0 {
            return 0;
        }
        let offset = u32::from(value.wrapping_sub(i16::MIN) as u16);
        let scaled = (u64::from(offset) * u64::from(extent - 1) + 32767) / 65535;
        scaled as u16
    }
    (axis(at.0, width), axis(at.1, height))
}

/// Convert a pixel position on a display `width` by `height` to a normalized
/// aim, or `None` when it falls outside the display.
///
/// The inverse of [`aim_to_pixels`], to within the rounding one pixel of a
/// 65536-step axis allows.
#[must_use]
pub fn aim_from_pixels(x: i32, y: i32, width: u32, height: u32) -> Option<(i16, i16)> {
    fn axis(value: i32, extent: u32) -> Option<i16> {
        if extent == 0 || value < 0 || value >= i32::try_from(extent).ok()? {
            return None;
        }
        if extent == 1 {
            return Some(0);
        }
        let scaled = (i64::from(value) * 65535) / i64::from(extent - 1);
        i16::try_from(scaled - 32768).ok()
    }
    Some((axis(x, width)?, axis(y, height)?))
}

#[cfg(test)]
mod aim_tests {
    use super::{aim_from_pixels, aim_to_pixels};

    /// The extremes land on the first and last pixel, and nothing lands off
    /// the end — a light gun aimed at the right edge must not index past the
    /// framebuffer.
    #[test]
    fn the_axis_spans_the_display_and_stops_at_its_edges() {
        for extent in [1u32, 2, 160, 256, 280, 288, 1024] {
            assert_eq!(aim_to_pixels((i16::MIN, i16::MIN), extent, extent).0, 0);
            assert_eq!(
                aim_to_pixels((i16::MAX, i16::MAX), extent, extent).0,
                (extent - 1) as u16,
                "extent {extent}"
            );
        }
    }

    /// Every pixel of a display round-trips to itself. This is what lets a
    /// host convert a cursor position once and a runtime convert it back
    /// without the aim drifting a pixel each way.
    #[test]
    fn every_pixel_round_trips() {
        for (width, height) in [(256u32, 192u32), (280, 240), (160, 144), (277, 288)] {
            for x in 0..width {
                for y in 0..height {
                    let aim = aim_from_pixels(x as i32, y as i32, width, height)
                        .expect("inside the display");
                    assert_eq!(
                        aim_to_pixels(aim, width, height),
                        (x as u16, y as u16),
                        "{width}x{height} at ({x}, {y})"
                    );
                }
            }
        }
    }

    /// Off the display is `None`, on every side. Clamping instead would turn
    /// "pointed away to reload" into a shot at the border.
    #[test]
    fn a_position_off_the_display_has_no_aim() {
        assert!(aim_from_pixels(-1, 10, 256, 192).is_none());
        assert!(aim_from_pixels(10, -1, 256, 192).is_none());
        assert!(aim_from_pixels(256, 10, 256, 192).is_none());
        assert!(aim_from_pixels(10, 192, 256, 192).is_none());
        assert!(aim_from_pixels(255, 191, 256, 192).is_some());

        // The bounds are their own guard, not a side effect of the
        // arithmetic overflowing: on a one-pixel axis nothing overflows, and
        // only the check keeps a position off it out.
        assert!(aim_from_pixels(0, 0, 1, 1).is_some());
        assert!(aim_from_pixels(1, 0, 1, 1).is_none());
        assert!(aim_from_pixels(0, 1, 1, 1).is_none());
    }

    /// A zero-sized display has no positions at all, rather than a degenerate
    /// one at the origin.
    #[test]
    fn a_display_with_no_pixels_has_no_positions() {
        assert!(aim_from_pixels(0, 0, 0, 0).is_none());
        assert_eq!(aim_to_pixels((0, 0), 0, 0), (0, 0));
    }
}
