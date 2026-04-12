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
    /// Relative pointer movement.
    PointerMotion {
        /// Stable device name, for example `"mouse-1"`.
        device: Cow<'static, str>,
        /// Horizontal movement delta.
        dx: i32,
        /// Vertical movement delta.
        dy: i32,
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
