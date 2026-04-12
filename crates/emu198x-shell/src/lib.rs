//! Cross-system shared infrastructure for Emu198x.
//!
//! This crate defines the narrow shared boundary above family runtimes and
//! below frontends, automation, capture, and orchestration layers. It does not
//! dictate how any machine runs internally.

pub mod capability;
pub mod error;
pub mod host;
pub mod machine;
pub mod media;
pub mod time;

pub use capability::{CapabilityId, CapabilitySet, known_capability};
pub use error::MachineError;
pub use host::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, InputEvent, NullAudioSink,
    NullFrameSink, NullTraceSink, PixelFormat, TraceEvent, TraceSink,
};
pub use machine::{
    Family, MachineCore, MachineId, MachineProfile, ProfileId, Region, ResetKind, RunResult,
    StopReason, SupportTier,
};
pub use media::{FirmwareRequirement, MediaImage, MediaKind, MediaSet, MediaSlot, WritebackPolicy};
pub use time::{ClockDesc, ClockRate, MachineTime};
