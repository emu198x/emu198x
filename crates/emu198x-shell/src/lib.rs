//! Cross-system shared infrastructure for Emu198x.
//!
//! This crate defines the narrow shared boundary above family runtimes and
//! below frontends, automation, capture, and orchestration layers. It does not
//! dictate how any machine runs internally.

pub mod asset;
pub mod audio;
pub mod capability;
pub mod capture;
pub mod control;
pub mod debug;
pub mod debug_info;
pub mod display;
pub mod error;
pub mod firmware;
pub mod headless;
pub mod host;
// Gamepad input is gilrs-backed throughout, and no browser build consumes
// it yet. A web gamepad layer would split the pure mapping types out; doing
// that now would be splitting for a caller that does not exist.
#[cfg(not(target_arch = "wasm32"))]
pub mod input;
pub mod keyboard;
pub mod machine;
pub mod mcp;
pub mod mcp_tools;
pub mod media;
pub mod query;
pub mod script;
pub mod session;
pub mod startup_media;
pub mod time;
pub mod video;
pub mod watch;

// Re-exported so the `impl_6502_debug_primitives!` macro can reach the shared
// disassembler as `$crate::isa_disasm` — keeping the dependency on this crate
// rather than every 6502 runtime. See 198x/decisions/rung1-wiring.md.
pub use isa_disasm;

pub use asset::{
    AssetLoadError, LoadedAsset, read_firmware_asset, read_media_asset, read_program_asset,
};
pub use audio::convert_audio_packet;
#[cfg(not(target_arch = "wasm32"))]
pub use audio::{NativeAudioError, NativeAudioOutput};
pub use capability::{CapabilityId, CapabilitySet, known_capability};
pub use capture::{
    AudioCapture, CaptureError, CapturedAudio, CapturedFrame, LatestFrameCapture, WavStreamWriter,
};
pub use control::{ControlCommand, MediaTransportAction, MediaTransportCommand};
pub use debug::{DebugPrimitives, DebugTarget, IoEvent};
pub use error::MachineError;
pub use firmware::{FirmwareImage, FirmwareSet};
pub use headless::{BootArtifacts, boot_machine, prepare_machine};
pub use host::{
    AudioPacket, AudioSink, FramePacket, FrameSink, HostIo, InputEvent, NullAudioSink,
    NullFrameSink, NullTraceSink, PixelFormat, TraceEvent, TraceSink, aim_from_pixels,
    aim_to_pixels,
};
#[cfg(not(target_arch = "wasm32"))]
pub use input::{
    AxisInputMap, AxisTarget, ButtonInputMap, ButtonTarget, HostAxis, HostControl,
    NativeGamepadInput,
};
pub use keyboard::{KeyTiming, KeyboardTarget, STANDARD_KEY_TIMING, StandardKeyboard};
pub use machine::{
    Family, FamilyRuntime, MachineCore, MachineId, MachineProfile, ProfileId, Region, ResetKind,
    RunResult, StopReason,
};
pub use media::{FirmwareRequirement, MediaImage, MediaKind, MediaSet, MediaSlot, WritebackPolicy};
pub use query::{
    NoAdditionalQueries, QueryError, QueryPathsResult, QueryResult, SESSION_QUERY_PATHS,
    SessionQueryProvider,
};
pub use script::{
    AyWriteEntry, DisasmInstruction, HeadlessScript, MemoryWriteEntry, ScriptError,
    ScriptMediaKind, ScriptMediaTransportAction, ScriptObservation, ScriptStep,
};
pub use session::{HeadlessSession, QueryBoolWaitResult, QueryTextWaitResult, SessionError};
pub use time::{ClockDesc, ClockRate, MachineTime};
pub use video::{
    DEFAULT_RECORDING_FADE_MS, VideoRecorder, VideoRecordingError, VideoRecordingSummary,
    compute_fps, find_ffmpeg, trim_audio_after, trim_audio_after_with_fade,
};
pub use watch::{WatchAyRecord, WatchError, WatchMemoryRecord, WatchMemorySource, WatchTarget};
