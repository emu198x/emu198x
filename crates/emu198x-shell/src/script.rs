//! Shared JSON script execution on top of one headless session.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::asset::{AssetLoadError, read_media_asset};
use crate::control::ControlCommand;
use crate::machine::{MachineCore, ResetKind};
use crate::media::{MediaImage, MediaKind, MediaSet};
use crate::query::{QueryError, QueryPathsResult, QueryResult, SessionQueryProvider};
use crate::session::{HeadlessSession, SessionError};

/// One user-facing script media kind with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaKind {
    Tape,
    Disk,
    Cartridge,
    Optical,
    Snapshot,
    Program,
}

impl From<ScriptMediaKind> for MediaKind {
    fn from(value: ScriptMediaKind) -> Self {
        match value {
            ScriptMediaKind::Tape => MediaKind::Tape,
            ScriptMediaKind::Disk => MediaKind::Disk,
            ScriptMediaKind::Cartridge => MediaKind::Cartridge,
            ScriptMediaKind::Optical => MediaKind::Optical,
            ScriptMediaKind::Snapshot => MediaKind::Snapshot,
            ScriptMediaKind::Program => MediaKind::Program,
        }
    }
}

/// One user-facing media transport action with stable JSON spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMediaTransportAction {
    Start,
    Stop,
}

impl From<ScriptMediaTransportAction> for crate::MediaTransportAction {
    fn from(value: ScriptMediaTransportAction) -> Self {
        match value {
            ScriptMediaTransportAction::Start => crate::MediaTransportAction::Start,
            ScriptMediaTransportAction::Stop => crate::MediaTransportAction::Stop,
        }
    }
}

/// One captured CPU write reported by [`ScriptObservation::WatchMemoryLog`].
///
/// Widened to `u32` on every field so the same shape covers both
/// 16-bit (Z80, 6502) and 32-bit (68000) address spaces. Per-system
/// binaries narrow on the way in (Spectrum truncates `pc` and `addr`
/// to `u16` and `value` to `u8` when matching).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteEntry {
    /// Program counter at the moment of the write.
    pub pc: u32,
    /// Target address of the write.
    pub addr: u32,
    /// Value that was written. Bytes occupy the low 8 bits; words
    /// occupy the low 16.
    pub value: u32,
}

/// One shared JSON script step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ScriptStep {
    /// Load one media image into a named slot.
    LoadMedia {
        /// Stable slot identifier.
        slot: String,
        /// User-facing JSON media kind.
        kind: ScriptMediaKind,
        /// Path to the media image on disk.
        path: PathBuf,
    },
    /// Start or stop media transport on a named slot.
    MediaTransport {
        /// Stable slot identifier.
        slot: String,
        /// Requested transport action.
        transport: ScriptMediaTransportAction,
    },
    /// Queue generic input events for the next run step.
    Input {
        /// The events to queue.
        events: Vec<crate::InputEvent>,
    },
    /// Run the machine for one number of native frames.
    RunFrames {
        /// Number of native video frames to execute.
        frames: u32,
    },
    /// Run native frames until `boot.detected = true`.
    WaitForBoot {
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Run native frames until one text-bearing query contains one substring.
    WaitForQueryContains {
        /// The query path to poll.
        path: String,
        /// The required substring.
        needle: String,
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Run native frames until one boolean query path reaches one target
    /// value.
    WaitForQueryBool {
        /// The query path to poll.
        path: String,
        /// The required boolean value.
        value: bool,
        /// Maximum number of native video frames to execute while waiting.
        max_frames: u32,
    },
    /// Resolve one shared query path.
    Query {
        /// The query path to resolve.
        path: String,
    },
    /// List supported query paths, optionally filtered by prefix.
    QueryPaths {
        /// Optional prefix filter.
        prefix: Option<String>,
    },
    /// Restore one snapshot file into the live machine.
    ///
    /// The shell crate's built-in executor decodes the runtime's own
    /// postcard save state. Per-system binaries that also handle
    /// portable formats intercept this step before delegation — the
    /// Spectrum binary, for example, dispatches `.sna` / `.z80` (and
    /// `.zip` archives wrapping one of those) through the appropriate
    /// format crate and falls back to the shell executor for postcard
    /// payloads. From the script author's perspective there is one
    /// step regardless of format; the binary chooses the parser from
    /// the file extension.
    LoadSnapshot {
        /// Path to the snapshot on disk.
        path: PathBuf,
    },
    /// Save the current machine snapshot to disk.
    SaveSnapshot {
        /// Output path for the snapshot.
        path: PathBuf,
    },
    /// Save the latest emitted frame as PNG.
    SaveScreenshot {
        /// Output path for the PNG file.
        path: PathBuf,
    },
    /// Save the captured audio stream as WAV.
    SaveAudioCapture {
        /// Output path for the WAV file.
        path: PathBuf,
        /// Whether to clear captured audio after writing the file.
        #[serde(default = "default_true")]
        reset_after: bool,
    },
    /// Switch the live machine to the named variant, loading its
    /// default ROM bundle from the conventional on-disk location.
    ///
    /// `machine` is a system-specific identifier (e.g.
    /// `"spectrum_48k"`, `"spectrum_128k"`) that the binary translates
    /// to its native machine kind and ROM-bundle resolver. The shell
    /// crate stays system-agnostic and surfaces this step via
    /// [`ScriptError::SystemSpecificStep`] when its built-in executor
    /// is asked to run it without a binary-side handler.
    ///
    /// Always resets in-progress state — loaded media, snapshots,
    /// frame counter, audio buffer. Use this as the first step of
    /// any script that targets a non-default variant.
    SetMachine {
        /// Snake-case variant identifier (binary-defined vocabulary).
        machine: String,
    },
    /// Wait for boot, then drive the BASIC editor to type `LOAD ""`
    /// and start tape transport on the named slot.
    ///
    /// Wraps the existing host-side `autoload_basic_tape` helper.
    /// System-specific (the helper currently lives in
    /// `runtime-sinclair-zx-spectrum`); same dispatch pattern as
    /// [`Self::SetMachine`].
    AutoloadTape {
        /// Stable slot identifier carrying the tape (e.g. `"tape-1"`).
        slot: String,
        /// Maximum number of native frames to spend waiting for boot
        /// before failing the autoload.
        max_boot_frames: u32,
    },
    /// Tokenise one plain-text `.bas` file and install it as the
    /// machine's current BASIC program.
    ///
    /// System-specific (the tokeniser and RAM-poke routines are per
    /// dialect); same dispatch pattern as [`Self::SetMachine`] —
    /// the binary intercepts this step before delegation. When `run`
    /// is `true` (the default), the binary also drives the editor's
    /// RUN keyword so the program starts executing.
    LoadBasicProgram {
        /// Path to the plain-text BASIC source file on disk.
        path: PathBuf,
        /// Whether to immediately RUN the program after installing it.
        #[serde(default = "default_true")]
        run: bool,
    },
    /// Begin recording the live framebuffer + audio to one MP4 file.
    ///
    /// Subsequent `RunFrames` / wait steps tee every emitted frame into the
    /// recorder. While a recording is active, `LoadSnapshot`, nested
    /// `StartVideoRecording`, and (binary-side) `SetMachine` are rejected
    /// because each would jump-cut the clip.
    StartVideoRecording {
        /// Final output path for the MP4 file.
        path: PathBuf,
    },
    /// Finalise the in-flight video recording.
    ///
    /// Closes the ffmpeg pipe, waits for the video pass, and (when audio has
    /// been captured) runs a second ffmpeg pass to mux audio into the final
    /// MP4. Emits [`ScriptObservation::StopVideoRecording`].
    StopVideoRecording,
    /// Begin recording emitted audio to a 16-bit PCM WAV file.
    ///
    /// Mirrors [`Self::StartVideoRecording`] for audio-only capture.
    /// Subsequent `run_frames` / wait steps tee audio into the
    /// session's capture buffer; the final WAV is written when
    /// [`Self::StopAudioRecording`] is called and contains only the
    /// samples emitted between the two calls.
    ///
    /// Use this instead of [`Self::SaveAudioCapture`] when the
    /// recording window is bounded by script steps rather than the
    /// whole session lifetime — no manual `reset_after` choreography,
    /// no leaked silence from frames before the chapter started.
    StartAudioRecording {
        /// Final output path for the WAV file.
        path: PathBuf,
    },
    /// Finalise the in-flight audio recording.
    ///
    /// Slices the audio capture buffer from the offset captured at
    /// `StartAudioRecording` to the current end, encodes the result
    /// as 16-bit PCM WAV, and writes it to disk. Emits
    /// [`ScriptObservation::StopAudioRecording`].
    StopAudioRecording,
    /// Query the AY-3-8912 chip's full register state in one call.
    ///
    /// Spectrum-family system-specific step (errors with
    /// [`ScriptError::SystemSpecificStep`] on the shell's built-in
    /// executor; the Spectrum binary intercepts it before delegation
    /// and returns [`ScriptObservation::QueryAy`] with the 16 raw
    /// registers plus decoded tone periods, mixer routing, amplitudes,
    /// noise period, and envelope. Errors when the active variant
    /// does not have an AY (16K / 48K / Spectrum+).
    ///
    /// Wraps the low-level `spectrum.ay.registers` /
    /// `spectrum.ay.selected_register` queries with named fields so
    /// curriculum scripts can assert on chip state without decoding
    /// the 16-byte raw array themselves.
    QueryAy,
    /// Reset the running machine.
    ///
    /// `kind = "hard"` is a power-cycle equivalent (machine state and
    /// RAM are reconstructed from firmware). `kind = "soft"` is a
    /// machine-local soft reset (intended to preserve RAM where the
    /// chipset supports it). Today most systems treat both kinds
    /// identically — the variant is plumbed end-to-end so per-system
    /// soft-reset semantics can land incrementally without changing
    /// the wire format.
    ///
    /// Clears session-side state (queued input, latest frame,
    /// captured audio, last run result). Rejected while a video
    /// recording is in flight, same rule as [`Self::LoadSnapshot`] —
    /// reset would jump-cut the clip.
    Reset {
        /// The kind of reset to perform.
        kind: ResetKind,
    },
    /// Read a contiguous span of CPU-visible memory.
    ///
    /// System-specific step (errors with
    /// [`ScriptError::SystemSpecificStep`] on the shell's built-in
    /// executor; per-system binaries intercept it and resolve through
    /// their machine's memory bus). Emits
    /// [`ScriptObservation::MemoryRead`] with the bytes read.
    MemoryRead {
        /// Start address (CPU-visible, low byte first).
        addr: u32,
        /// Number of bytes to read. Implementations may cap at 256.
        len: u32,
    },
    /// Write one byte to CPU-visible memory.
    ///
    /// System-specific step (binary-dispatched). Silent — emits no
    /// observation.
    PokeByte {
        /// Target address.
        addr: u32,
        /// Byte to write.
        value: u8,
    },
    /// Write one 16-bit word to CPU-visible memory.
    ///
    /// System-specific step (binary-dispatched). The write order is
    /// system-defined: Spectrum/Z80 writes little-endian (low byte at
    /// `addr`); 68000-class systems write big-endian. Silent.
    PokeWord {
        /// Target address.
        addr: u32,
        /// 16-bit value to write.
        value: u16,
    },
    /// Begin recording CPU writes inside the half-open address range
    /// `[addr, addr + len)`. Replaces any prior watch range and clears
    /// the captured log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryStart`].
    WatchMemoryStart {
        /// Watch range start address.
        addr: u32,
        /// Watch range length in bytes (must be ≥ 1).
        len: u32,
    },
    /// Stop watching and drop both the range and the captured log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryClear`] with the count of
    /// records that were dropped.
    WatchMemoryClear,
    /// Fetch the captured write log.
    ///
    /// System-specific (binary-dispatched). Emits
    /// [`ScriptObservation::WatchMemoryLog`] with up to `limit`
    /// most-recent entries (default 64).
    WatchMemoryLog {
        /// Maximum number of entries to return. Defaults to 64.
        #[serde(default)]
        limit: Option<u32>,
        /// When `true`, deduplicate identical `(pc, addr, value)`
        /// triples before applying the limit.
        #[serde(default)]
        unique: bool,
    },
}

/// One JSON script made of ordered shared steps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeadlessScript {
    /// The ordered steps to execute.
    pub steps: Vec<ScriptStep>,
}

/// One structured observation emitted by the shared script layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptObservation {
    /// Result of a frame-run step.
    RunFrames {
        /// Number of requested native frames.
        frames: u32,
        /// Machine time reached after the run.
        reached: crate::MachineTime,
        /// Why the machine stopped.
        stop_reason: crate::StopReason,
    },
    /// Result of waiting for boot detection.
    WaitForBoot {
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when boot was detected.
        reached: crate::MachineTime,
        /// Human-readable boot status note.
        reason: String,
        /// Optional decoded text row reported by `boot.row`.
        row: Option<u64>,
    },
    /// Result of waiting for one text-bearing query to contain one substring.
    WaitForQueryContains {
        /// The query path that matched.
        path: String,
        /// The required substring.
        needle: String,
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when the wait completed.
        reached: crate::MachineTime,
        /// Matching line index when the query returned an array of strings.
        line: Option<u64>,
        /// The actual matching line or string.
        matched_text: String,
    },
    /// Result of waiting for one boolean query to reach one target value.
    WaitForQueryBool {
        /// The query path that matched.
        path: String,
        /// The required boolean value.
        value: bool,
        /// Number of native frames executed while waiting.
        frames: u32,
        /// Machine time reached when the wait completed.
        reached: crate::MachineTime,
    },
    /// Result of resolving one query path.
    Query {
        /// Resolved query data.
        result: QueryResult,
    },
    /// Result of listing supported query paths.
    QueryPaths {
        /// Query-path listing response.
        result: QueryPathsResult,
    },
    /// Result of switching the live machine.
    SetMachine {
        /// The variant identifier requested by the script.
        machine: String,
        /// Resolved profile id reached after the switch.
        profile_id: String,
        /// Resolved display name reached after the switch.
        display_name: String,
    },
    /// Result of an autoload-tape sequence.
    AutoloadTape {
        /// Slot the tape was autoloaded from.
        slot: String,
        /// Number of native frames spent waiting for boot.
        boot_frames: u32,
    },
    /// Result of installing one tokenised BASIC program.
    LoadBasicProgram {
        /// Tokenised program length in bytes.
        program_bytes: u16,
        /// Whether the binary drove the editor to `RUN` the program
        /// after installing it.
        ran: bool,
    },
    /// Result of finalising one video recording.
    StopVideoRecording {
        /// Final MP4 file path.
        path: PathBuf,
        /// Total frames captured during the recording window.
        frames: u64,
        /// Approximate duration of the captured clip in milliseconds.
        duration_ms: u64,
        /// Whether the final MP4 contains a muxed audio track.
        has_audio: bool,
    },
    /// Result of querying the AY-3-8912 chip state.
    QueryAy {
        /// Last register index selected by an `OUT (#FFFD)` write.
        /// Reads from `IN (#FFFD)` return `raw[selected_register]`.
        selected_register: u8,
        /// All 16 registers, post-mask. `raw[0..2]` is tone-A period
        /// (12-bit, low byte first); `raw[2..4]` tone-B; `raw[4..6]`
        /// tone-C; `raw[6]` noise period (5-bit); `raw[7]` mixer;
        /// `raw[8..11]` amplitudes A/B/C; `raw[11..13]` envelope
        /// period; `raw[13]` envelope shape; `raw[14..16]` I/O ports.
        raw: Vec<u8>,
        /// Tone-A period (12-bit value built from R0 + R1).
        tone_period_a: u16,
        /// Tone-B period (R2 + R3).
        tone_period_b: u16,
        /// Tone-C period (R4 + R5).
        tone_period_c: u16,
        /// Noise period (5-bit, R6).
        noise_period: u8,
        /// Mixer register (R7). Bits 0..2 disable tone channels
        /// A/B/C; bits 3..5 disable noise; bits 6..7 set port direction.
        mixer: u8,
        /// Channel-A amplitude (R8). Bit 4 selects envelope mode;
        /// bits 0..3 are fixed amplitude.
        amplitude_a: u8,
        /// Channel-B amplitude (R9).
        amplitude_b: u8,
        /// Channel-C amplitude (R10).
        amplitude_c: u8,
        /// Envelope period (16-bit value built from R11 + R12).
        envelope_period: u16,
        /// Envelope shape (R13, 4-bit).
        envelope_shape: u8,
    },
    /// Result of finalising one standalone audio recording.
    StopAudioRecording {
        /// Final WAV file path.
        path: PathBuf,
        /// Per-channel sample count.
        samples: u64,
        /// Sample rate of the captured stream, in Hz.
        sample_rate: u32,
        /// Channel count of the captured stream.
        channels: u8,
        /// Approximate clip duration in milliseconds.
        duration_ms: u64,
    },
    /// Result of a reset step. The `kind` field on
    /// [`ScriptObservation`] is the tag itself (`"reset"`), so the
    /// performed reset kind is reported under `performed` to avoid
    /// the tag-name collision.
    Reset {
        /// The kind of reset that was performed.
        performed: ResetKind,
        /// Machine time after the reset (typically zero).
        reached: crate::MachineTime,
    },
    /// Result of a memory-read step.
    MemoryRead {
        /// Start address that was read.
        addr: u32,
        /// Number of bytes that were read (may be capped below the
        /// requested length).
        len: u32,
        /// Raw bytes in memory order.
        bytes: Vec<u8>,
    },
    /// Result of starting a memory write watch.
    WatchMemoryStart {
        /// Watch range start.
        addr: u32,
        /// Watch range length.
        len: u32,
        /// Capacity (max records the log can hold). Once full the log
        /// stops growing; callers should poll via
        /// [`ScriptStep::WatchMemoryLog`] before that.
        capacity: u32,
    },
    /// Result of stopping a memory write watch and dropping its log.
    WatchMemoryClear {
        /// `true` when a watch range was configured before the clear.
        had_watch: bool,
        /// Number of records captured between start and clear.
        captured: u32,
    },
    /// Result of fetching the memory write log.
    WatchMemoryLog {
        /// Current watch range start, or `None` if no watch is active.
        addr: Option<u32>,
        /// Current watch range length, or `None` if no watch is active.
        len: Option<u32>,
        /// Total number of records currently held.
        total_writes: u32,
        /// Number of records actually returned (after limit + unique).
        returned: u32,
        /// Most-recent entries up to the requested limit, in capture
        /// order (oldest first).
        entries: Vec<MemoryWriteEntry>,
    },
}

impl HeadlessScript {
    /// Parses one script from UTF-8 JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid.
    pub fn from_json_str(text: &str) -> Result<Self, ScriptError> {
        Ok(serde_json::from_str(text)?)
    }

    /// Loads one script from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O or JSON parsing fails.
    pub fn from_path(path: &Path) -> Result<Self, ScriptError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json_str(&text)
    }

    /// Executes this script against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes this script and returns any structured observations it emits.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<Vec<ScriptObservation>, ScriptError> {
        let mut observations = Vec::new();
        for step in &self.steps {
            if let Some(observation) = step.execute_collect(session)? {
                observations.push(observation);
            }
        }

        Ok(observations)
    }
}

impl ScriptStep {
    /// Executes one script step against one live headless session.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, or capture output fails.
    pub fn execute<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<(), ScriptError> {
        self.execute_collect(session).map(|_| ())
    }

    /// Executes one script step and returns any structured observation it
    /// produces.
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O, machine control, query resolution, or
    /// capture output fails.
    pub fn execute_collect<M: MachineCore, Q: SessionQueryProvider<M>>(
        &self,
        session: &mut HeadlessSession<M, Q>,
    ) -> Result<Option<ScriptObservation>, ScriptError> {
        match self {
            Self::LoadMedia { slot, kind, path } => {
                let loaded = read_media_asset(path, (*kind).into())?;
                let mut media = MediaSet::new();
                media.push(MediaImage::new(slot.clone(), (*kind).into(), &loaded.bytes));
                session.load_media(&media)?;
                Ok(None)
            }
            Self::MediaTransport { slot, transport } => {
                session.command(&ControlCommand::MediaTransport(
                    crate::MediaTransportCommand::new(slot.clone(), (*transport).into()),
                ))?;
                Ok(None)
            }
            Self::Input { events } => {
                session.queue_inputs(events.iter().cloned());
                Ok(None)
            }
            Self::RunFrames { frames } => {
                let result = session.run_frames(*frames)?;
                Ok(Some(ScriptObservation::RunFrames {
                    frames: *frames,
                    reached: result.reached,
                    stop_reason: result.stop_reason,
                }))
            }
            Self::WaitForBoot { max_frames } => {
                let result = session.wait_for_boot(*max_frames)?;
                Ok(Some(ScriptObservation::WaitForBoot {
                    frames: result.frames,
                    reached: result.reached,
                    reason: result.reason,
                    row: result.row,
                }))
            }
            Self::WaitForQueryContains {
                path,
                needle,
                max_frames,
            } => {
                let result = session.wait_for_query_text_contains(path, needle, *max_frames)?;
                Ok(Some(ScriptObservation::WaitForQueryContains {
                    path: result.path,
                    needle: result.needle,
                    frames: result.frames,
                    reached: result.reached,
                    line: result.line,
                    matched_text: result.matched_text,
                }))
            }
            Self::WaitForQueryBool {
                path,
                value,
                max_frames,
            } => {
                let result = session.wait_for_query_bool(path, *value, *max_frames)?;
                Ok(Some(ScriptObservation::WaitForQueryBool {
                    path: result.path,
                    value: result.expected,
                    frames: result.frames,
                    reached: result.reached,
                }))
            }
            Self::Query { path } => {
                let result = session.query(path)?;
                Ok(Some(ScriptObservation::Query { result }))
            }
            Self::QueryPaths { prefix } => {
                let result = session.query_paths(prefix.as_deref());
                Ok(Some(ScriptObservation::QueryPaths { result }))
            }
            Self::LoadSnapshot { path } => {
                let bytes = std::fs::read(path)?;
                session.restore_snapshot(&bytes)?;
                Ok(None)
            }
            Self::SaveSnapshot { path } => {
                session.save_snapshot(path)?;
                Ok(None)
            }
            Self::SaveScreenshot { path } => {
                session.save_screenshot(path)?;
                Ok(None)
            }
            Self::SaveAudioCapture { path, reset_after } => {
                session.save_audio_capture(path)?;
                if *reset_after {
                    session.clear_audio_capture();
                }
                Ok(None)
            }
            Self::SetMachine { .. } => Err(ScriptError::SystemSpecificStep {
                step: "set_machine",
            }),
            Self::QueryAy => Err(ScriptError::SystemSpecificStep { step: "query_ay" }),
            Self::AutoloadTape { .. } => Err(ScriptError::SystemSpecificStep {
                step: "autoload_tape",
            }),
            Self::LoadBasicProgram { .. } => Err(ScriptError::SystemSpecificStep {
                step: "load_basic_program",
            }),
            Self::MemoryRead { .. } => Err(ScriptError::SystemSpecificStep {
                step: "memory_read",
            }),
            Self::PokeByte { .. } => Err(ScriptError::SystemSpecificStep { step: "poke_byte" }),
            Self::PokeWord { .. } => Err(ScriptError::SystemSpecificStep { step: "poke_word" }),
            Self::WatchMemoryStart { .. } => Err(ScriptError::SystemSpecificStep {
                step: "watch_memory_start",
            }),
            Self::WatchMemoryClear => Err(ScriptError::SystemSpecificStep {
                step: "watch_memory_clear",
            }),
            Self::WatchMemoryLog { .. } => Err(ScriptError::SystemSpecificStep {
                step: "watch_memory_log",
            }),
            Self::StartVideoRecording { path } => {
                session.start_video_recording(path.clone())?;
                Ok(None)
            }
            Self::StopVideoRecording => {
                let summary = session.stop_video_recording()?;
                Ok(Some(ScriptObservation::StopVideoRecording {
                    path: summary.path,
                    frames: summary.frames,
                    duration_ms: summary.duration_ms,
                    has_audio: summary.has_audio,
                }))
            }
            Self::Reset { kind } => {
                session.reset(*kind)?;
                Ok(Some(ScriptObservation::Reset {
                    performed: *kind,
                    reached: session.time(),
                }))
            }
            Self::StartAudioRecording { path } => {
                session.start_audio_recording(path.clone())?;
                Ok(None)
            }
            Self::StopAudioRecording => {
                let summary = session.stop_audio_recording()?;
                Ok(Some(ScriptObservation::StopAudioRecording {
                    path: summary.path,
                    samples: summary.samples as u64,
                    sample_rate: summary.sample_rate,
                    channels: summary.channels,
                    duration_ms: summary.duration_ms,
                }))
            }
        }
    }
}

/// Error surfaced by the shared JSON script layer.
#[derive(Debug, Error)]
pub enum ScriptError {
    /// Asset loading or archive extraction failed.
    #[error(transparent)]
    Asset(#[from] AssetLoadError),

    /// One filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON parsing failed.
    #[error(transparent)]
    Parse(#[from] serde_json::Error),

    /// Session execution failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Query resolution failed.
    #[error(transparent)]
    Query(#[from] QueryError),

    /// One step requires a binary-side handler the shell crate does
    /// not own (e.g. `SetMachine`, `AutoloadTape`). Per-system binaries
    /// intercept these steps before delegating to the shell executor.
    #[error("script step `{step}` requires a system-specific handler")]
    SystemSpecificStep {
        /// The step's serde tag (e.g. `"set_machine"`, `"autoload_tape"`).
        step: &'static str,
    },
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::error::MachineError;
    use crate::host::{AudioPacket, FramePacket, HostIo, PixelFormat};
    use crate::machine::{
        Family, MachineId, MachineProfile, ProfileId, Region, ResetKind, RunResult, StopReason,
        SupportTier,
    };
    use crate::media::{FirmwareRequirement, MediaSlot, WritebackPolicy};
    use crate::query::SessionQueryProvider;
    use crate::time::{ClockDesc, ClockRate, MachineTime};
    use serde_json::json;

    struct DummyMachine {
        profile: MachineProfile,
        time: MachineTime,
        tape_loaded: usize,
        commands: usize,
        restored: usize,
    }

    impl DummyMachine {
        fn new() -> Self {
            Self {
                profile: MachineProfile {
                    machine_id: MachineId::from("dummy-machine"),
                    profile_id: ProfileId::from("dummy-profile"),
                    display_name: "Dummy".into(),
                    family: Family::Spectrum,
                    region: Region::Pal,
                    support_tier: SupportTier::Research,
                    release_year: 1982,
                    summary: "dummy".into(),
                    clock: ClockDesc::new("master-cycle", ClockRate::from_hz(1)),
                    firmware: vec![FirmwareRequirement::new("rom-0", "ROM 0", false)],
                    media_slots: vec![MediaSlot::new(
                        "tape-1",
                        "Tape Deck",
                        MediaKind::Tape,
                        false,
                        WritebackPolicy::InMemoryOnly,
                    )],
                    capabilities: CapabilitySet::new(),
                },
                time: MachineTime::default(),
                tape_loaded: 0,
                commands: 0,
                restored: 0,
            }
        }
    }

    impl MachineCore for DummyMachine {
        fn profile(&self) -> &MachineProfile {
            &self.profile
        }

        fn time(&self) -> MachineTime {
            self.time
        }

        fn reset(&mut self, _kind: ResetKind) {}

        fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
            self.tape_loaded += media.images.len();
            Ok(())
        }

        fn run_until(
            &mut self,
            target: MachineTime,
            host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            self.time = target;
            host.frame_sink.push_frame(FramePacket {
                timestamp: target,
                format: PixelFormat::Indexed8,
                width: 1,
                height: 1,
                palette: Some(&[0x000000FF, 0xFFFFFFFF]),
                pixels: &[1],
            })?;
            host.audio_sink.push_audio(AudioPacket {
                timestamp: target,
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0, 0.25],
            })?;
            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }

        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![0x55])
        }

        fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
            self.restored += 1;
            Ok(())
        }

        fn command(&mut self, _command: &ControlCommand) -> Result<(), MachineError> {
            self.commands += 1;
            Ok(())
        }

        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::new()
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct DummyQueryProvider;

    impl SessionQueryProvider<DummyMachine> for DummyQueryProvider {
        fn query(
            &self,
            machine: &DummyMachine,
            path: &str,
        ) -> Result<Option<QueryResult>, QueryError> {
            match path {
                "boot.detected" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get() >= 3 * 69_888),
                })),
                "boot.reason" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 3 * 69_888 {
                        "dummy boot banner is visible"
                    } else {
                        "dummy boot banner not visible yet"
                    }),
                })),
                "boot.row" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 3 * 69_888 {
                        Some(23u64)
                    } else {
                        None::<u64>
                    }),
                })),
                "dummy.flag" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get() >= 2 * 69_888),
                })),
                "screen.text.lines" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 4 * 69_888 {
                        vec!["READY".to_owned(), "MANIC MINER".to_owned()]
                    } else {
                        vec!["READY".to_owned(), "LOADING".to_owned()]
                    }),
                })),
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn headless_script_parses_json_array() {
        let script = HeadlessScript::from_json_str(
            r#"
            [
              {"action":"run_frames","frames":2},
              {"action":"query","path":"session.time"},
              {"action":"save_screenshot","path":"boot.png"}
            ]
            "#,
        )
        .expect("script json should parse");

        assert_eq!(
            script.steps,
            vec![
                ScriptStep::RunFrames { frames: 2 },
                ScriptStep::Query {
                    path: "session.time".to_owned()
                },
                ScriptStep::SaveScreenshot {
                    path: PathBuf::from("boot.png")
                }
            ]
        );
    }

    #[test]
    fn headless_script_executes_media_run_capture_and_snapshot_steps() {
        let temp_dir = std::env::temp_dir();
        let media_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-demo.tap",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-shot.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-audio.wav",
            std::process::id()
        ));
        let snapshot_path = temp_dir.join(format!(
            "emu198x-shell-script-{}-state.pst",
            std::process::id()
        ));
        std::fs::write(&media_path, [0x13, 0x00, 0x00]).expect("media fixture should write");

        let script = HeadlessScript {
            steps: vec![
                ScriptStep::LoadMedia {
                    slot: "tape-1".to_owned(),
                    kind: ScriptMediaKind::Tape,
                    path: media_path.clone(),
                },
                ScriptStep::MediaTransport {
                    slot: "tape-1".to_owned(),
                    transport: ScriptMediaTransportAction::Start,
                },
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::SaveScreenshot {
                    path: screenshot_path.clone(),
                },
                ScriptStep::SaveAudioCapture {
                    path: audio_path.clone(),
                    reset_after: true,
                },
                ScriptStep::SaveSnapshot {
                    path: snapshot_path.clone(),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should run to completion");

        assert_eq!(session.machine().tape_loaded, 1);
        assert_eq!(session.machine().commands, 1);
        assert_eq!(
            observations,
            vec![ScriptObservation::RunFrames {
                frames: 1,
                reached: MachineTime::new(69888),
                stop_reason: StopReason::ReachedTarget,
            }]
        );
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());
        assert!(snapshot_path.is_file());

        let _ = std::fs::remove_file(media_path);
        let _ = std::fs::remove_file(screenshot_path);
        let _ = std::fs::remove_file(audio_path);
        let _ = std::fs::remove_file(snapshot_path);
    }

    #[test]
    fn headless_script_collects_query_observations() {
        let script = HeadlessScript {
            steps: vec![
                ScriptStep::RunFrames { frames: 1 },
                ScriptStep::Query {
                    path: "session.time".to_owned(),
                },
                ScriptStep::QueryPaths {
                    prefix: Some("capture.".to_owned()),
                },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should produce observations");

        assert_eq!(observations.len(), 3);
        assert_eq!(
            observations[1],
            ScriptObservation::Query {
                result: QueryResult {
                    path: "session.time".to_owned(),
                    value: serde_json::json!(69888),
                }
            }
        );
        assert_eq!(
            observations[2],
            ScriptObservation::QueryPaths {
                result: QueryPathsResult {
                    prefix: Some("capture.".to_owned()),
                    paths: vec![
                        "capture.has_audio".to_owned(),
                        "capture.has_frame".to_owned()
                    ],
                }
            }
        );
    }

    #[test]
    fn headless_script_waits_for_boot_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForBoot { max_frames: 3 }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy boot");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForBoot {
                frames: 3,
                reached: MachineTime::new(209_664),
                reason: "dummy boot banner is visible".to_owned(),
                row: Some(23),
            }]
        );
    }

    #[test]
    fn headless_script_waits_for_query_text_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForQueryContains {
                path: "screen.text.lines".to_owned(),
                needle: "MANIC MINER".to_owned(),
                max_frames: 4,
            }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy title text");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForQueryContains {
                path: "screen.text.lines".to_owned(),
                needle: "MANIC MINER".to_owned(),
                frames: 4,
                reached: MachineTime::new(279_552),
                line: Some(1),
                matched_text: "MANIC MINER".to_owned(),
            }]
        );
    }

    #[test]
    fn headless_script_waits_for_query_bool_and_reports_result() {
        let script = HeadlessScript {
            steps: vec![ScriptStep::WaitForQueryBool {
                path: "dummy.flag".to_owned(),
                value: true,
                max_frames: 2,
            }],
        };

        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let observations = script
            .execute_collect(&mut session)
            .expect("script should wait for dummy boolean query");

        assert_eq!(
            observations,
            vec![ScriptObservation::WaitForQueryBool {
                path: "dummy.flag".to_owned(),
                value: true,
                frames: 2,
                reached: MachineTime::new(139_776),
            }]
        );
    }

    #[test]
    fn set_machine_round_trips_through_json() {
        let json = r#"[{"action":"set_machine","machine":"spectrum_48k"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::SetMachine {
                machine: "spectrum_48k".to_owned(),
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(
            serialized,
            r#"[{"action":"set_machine","machine":"spectrum_48k"}]"#
        );
    }

    #[test]
    fn set_machine_step_returns_system_specific_error_from_shell_executor() {
        // Shell-level executor stays system-agnostic; the binary
        // intercepts SetMachine before delegating. Calling the shell
        // executor directly surfaces the error the binary uses to
        // detect "this is one of mine".
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::SetMachine {
            machine: "spectrum_48k".to_owned(),
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => assert_eq!(step, "set_machine"),
            other => panic!("expected SystemSpecificStep error, got {other:?}"),
        }
    }

    #[test]
    fn autoload_tape_round_trips_through_json() {
        let json = r#"[{"action":"autoload_tape","slot":"tape-1","max_boot_frames":250}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::AutoloadTape {
                slot: "tape-1".to_owned(),
                max_boot_frames: 250,
            }]
        );
    }

    #[test]
    fn autoload_tape_step_returns_system_specific_error_from_shell_executor() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::AutoloadTape {
            slot: "tape-1".to_owned(),
            max_boot_frames: 100,
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => assert_eq!(step, "autoload_tape"),
            other => panic!("expected SystemSpecificStep error, got {other:?}"),
        }
    }

    #[test]
    fn load_basic_program_round_trips_through_json() {
        let json = r#"[{"action":"load_basic_program","path":"hello.bas","run":true}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::LoadBasicProgram {
                path: PathBuf::from("hello.bas"),
                run: true,
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn load_basic_program_defaults_run_to_true_when_omitted() {
        let json = r#"[{"action":"load_basic_program","path":"hello.bas"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::LoadBasicProgram {
                path: PathBuf::from("hello.bas"),
                run: true,
            }]
        );
    }

    #[test]
    fn load_basic_program_step_returns_system_specific_error_from_shell_executor() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );
        let step = ScriptStep::LoadBasicProgram {
            path: PathBuf::from("hello.bas"),
            run: true,
        };
        match step.execute_collect(&mut session) {
            Err(ScriptError::SystemSpecificStep { step }) => {
                assert_eq!(step, "load_basic_program");
            }
            other => panic!("expected SystemSpecificStep, got {other:?}"),
        }
    }

    #[test]
    fn load_basic_program_observation_serializes_summary_fields() {
        let observation = ScriptObservation::LoadBasicProgram {
            program_bytes: 42,
            ran: true,
        };
        let json = serde_json::to_string(&observation).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"load_basic_program","program_bytes":42,"ran":true}"#
        );
    }

    #[test]
    fn start_video_recording_round_trips_through_json() {
        let json = r#"[{"action":"start_video_recording","path":"clip.mp4"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(
            script.steps,
            vec![ScriptStep::StartVideoRecording {
                path: PathBuf::from("clip.mp4"),
            }]
        );
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn stop_video_recording_round_trips_through_json() {
        let json = r#"[{"action":"stop_video_recording"}]"#;
        let script = HeadlessScript::from_json_str(json).expect("script should parse");
        assert_eq!(script.steps, vec![ScriptStep::StopVideoRecording]);
        let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
        assert_eq!(serialized, json);
    }

    #[test]
    fn stop_video_recording_observation_serializes_the_summary_fields() {
        let observation = ScriptObservation::StopVideoRecording {
            path: PathBuf::from("/tmp/clip.mp4"),
            frames: 250,
            duration_ms: 5_000,
            has_audio: true,
        };
        let json = serde_json::to_string(&observation).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"stop_video_recording","path":"/tmp/clip.mp4","frames":250,"duration_ms":5000,"has_audio":true}"#
        );
    }

    #[test]
    fn set_machine_observation_uses_machine_field_to_avoid_kind_tag_clash() {
        // The ScriptObservation enum is internally tagged with
        // `kind: <variant_name>`. A field literally named `kind`
        // collides; we use `machine` instead. This regression test
        // freezes the JSON shape so a future "rename for symmetry"
        // edit doesn't accidentally break Code198x's report parsers.
        let observation = ScriptObservation::SetMachine {
            machine: "spectrum_48k".to_owned(),
            profile_id: "sinclair-zx-spectrum-48k-pal".to_owned(),
            display_name: "ZX Spectrum 48K (PAL)".to_owned(),
        };
        let json = serde_json::to_string(&observation).expect("serialize observation");
        assert_eq!(
            json,
            r#"{"kind":"set_machine","machine":"spectrum_48k","profile_id":"sinclair-zx-spectrum-48k-pal","display_name":"ZX Spectrum 48K (PAL)"}"#
        );
    }

    #[test]
    fn audio_recording_round_trips_through_json() {
        for json in [
            r#"[{"action":"start_audio_recording","path":"out.wav"}]"#,
            r#"[{"action":"stop_audio_recording"}]"#,
        ] {
            let script = HeadlessScript::from_json_str(json).expect("script should parse");
            let reserialised = serde_json::to_string(&script.steps).expect("re-serialize");
            assert_eq!(reserialised, json);
        }
    }

    #[test]
    fn reset_step_round_trips_through_json_for_both_kinds() {
        for (json, kind) in [
            (
                r#"[{"action":"reset","kind":"hard"}]"#,
                ResetKind::Hard,
            ),
            (
                r#"[{"action":"reset","kind":"soft"}]"#,
                ResetKind::Soft,
            ),
        ] {
            let script = HeadlessScript::from_json_str(json).expect("script should parse");
            assert_eq!(script.steps, vec![ScriptStep::Reset { kind }]);
            let serialized = serde_json::to_string(&script.steps).expect("re-serialize");
            assert_eq!(serialized, json);
        }
    }

    #[test]
    fn reset_observation_uses_performed_field_to_avoid_kind_tag_clash() {
        // Same constraint as set_machine: ScriptObservation tags with
        // `kind`, so the variant uses `performed` for the reset kind.
        let observation = ScriptObservation::Reset {
            performed: ResetKind::Soft,
            reached: MachineTime::new(0),
        };
        let json = serde_json::to_string(&observation).expect("serialize observation");
        assert_eq!(
            json,
            r#"{"kind":"reset","performed":"soft","reached":0}"#
        );
    }

    #[test]
    fn headless_script_executes_reset_and_emits_observation() {
        let script = HeadlessScript {
            steps: vec![
                ScriptStep::RunFrames { frames: 2 },
                ScriptStep::Reset { kind: ResetKind::Hard },
                ScriptStep::Reset { kind: ResetKind::Soft },
            ],
        };

        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let observations = script
            .execute_collect(&mut session)
            .expect("script should run to completion");

        assert_eq!(
            observations,
            vec![
                ScriptObservation::RunFrames {
                    frames: 2,
                    reached: MachineTime::new(2 * 69888),
                    stop_reason: StopReason::ReachedTarget,
                },
                ScriptObservation::Reset {
                    performed: ResetKind::Hard,
                    reached: MachineTime::new(2 * 69888),
                },
                ScriptObservation::Reset {
                    performed: ResetKind::Soft,
                    reached: MachineTime::new(2 * 69888),
                },
            ]
        );
    }
}
