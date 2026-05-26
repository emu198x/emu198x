//! Shared headless session state above one machine runtime.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::capture::{AudioCapture, CaptureError, CapturedFrame, LatestFrameCapture};
use crate::control::ControlCommand;
use crate::error::MachineError;
use crate::headless::prepare_machine;
use crate::host::{HostIo, InputEvent, NullTraceSink, TraceSink};
use crate::machine::{MachineCore, ResetKind, RunResult, StopReason};
use crate::media::MediaSet;
use crate::query::{
    NoAdditionalQueries, QueryError, QueryPathsResult, QueryResult, SessionQueryProvider,
    query_paths, query_value,
};
use crate::time::MachineTime;
use crate::video::{
    VideoRecorder, VideoRecordingError, VideoRecordingSummary, compute_fps, trim_audio_after,
};

/// Error surfaced by shared headless session helpers.
#[derive(Debug, Error)]
pub enum SessionError {
    /// One machine operation failed.
    #[error(transparent)]
    Machine(#[from] MachineError),

    /// One query resolution failed.
    #[error(transparent)]
    Query(#[from] QueryError),

    /// One capture conversion failed.
    #[error(transparent)]
    Capture(#[from] CaptureError),

    /// One filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// One query path resolved to an unexpected JSON value shape.
    #[error("query path {path} returned a value that is not {expected}")]
    UnexpectedQueryValue {
        /// The path that resolved to the wrong shape.
        path: String,
        /// The expected JSON value shape.
        expected: &'static str,
    },

    /// Boot was not detected within the requested frame budget.
    #[error("boot was not detected within {max_frames} frames: {reason}")]
    BootTimeout {
        /// Maximum number of frames the wait helper was allowed to run.
        max_frames: u32,
        /// The last reported boot reason, when available.
        reason: String,
    },

    /// One text-bearing query path did not produce the requested match.
    #[error("query path {path} did not contain {needle:?} within {max_frames} frames")]
    QueryTextTimeout {
        /// The text-bearing query path that was polled.
        path: String,
        /// The requested substring.
        needle: String,
        /// Maximum number of frames the wait helper was allowed to run.
        max_frames: u32,
    },

    /// One boolean query path did not reach the requested state.
    #[error("query path {path} did not become {expected} within {max_frames} frames")]
    QueryBoolTimeout {
        /// The boolean query path that was polled.
        path: String,
        /// The requested boolean value.
        expected: bool,
        /// Maximum number of frames the wait helper was allowed to run.
        max_frames: u32,
    },

    /// One video recording operation failed.
    #[error(transparent)]
    Video(#[from] VideoRecordingError),

    /// One audio recording operation failed.
    #[error(transparent)]
    Audio(#[from] AudioRecordingError),

    /// One state-mutating operation was rejected because a video recording
    /// is in flight (e.g. snapshot restore would jump-cut the clip).
    #[error("operation `{operation}` is not allowed while video recording is active")]
    DisallowedDuringRecording {
        /// The rejected operation's stable name.
        operation: &'static str,
    },
}

/// Errors surfaced by the standalone audio recorder.
#[derive(Debug, thiserror::Error)]
pub enum AudioRecordingError {
    /// `start_audio_recording` was called while a recording was
    /// already in flight.
    #[error("audio recording is already in flight")]
    AlreadyRecording,
    /// `stop_audio_recording` was called with no recording in flight.
    #[error("no audio recording in flight")]
    NotRecording,
    /// No audio samples landed in the recording window. Either no
    /// `run_frames` ran between start and stop, or the variant emitted
    /// no audio packets in that interval.
    #[error("audio recording window captured zero samples")]
    NoAudio,
}

/// Result of waiting for one machine to report `boot.detected = true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootWaitResult {
    /// Number of native frames executed while waiting.
    pub frames: u32,
    /// Machine time reached when the wait completed.
    pub reached: MachineTime,
    /// Human-readable boot status note from `boot.reason`.
    pub reason: String,
    /// Optional decoded text row reported by `boot.row`.
    pub row: Option<u64>,
}

/// Result of waiting for one text-bearing query to contain one substring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryTextWaitResult {
    /// The query path that matched.
    pub path: String,
    /// The requested substring.
    pub needle: String,
    /// Number of native frames executed while waiting.
    pub frames: u32,
    /// Machine time reached when the wait completed.
    pub reached: MachineTime,
    /// Matching line index when the query returned an array of strings.
    pub line: Option<u64>,
    /// The actual matched text fragment container.
    pub matched_text: String,
}

/// Result of waiting for one boolean query path to reach one target value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBoolWaitResult {
    /// The query path that matched.
    pub path: String,
    /// The requested boolean value.
    pub expected: bool,
    /// Number of native frames executed while waiting.
    pub frames: u32,
    /// Machine time reached when the wait completed.
    pub reached: MachineTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BootQueryState {
    detected: bool,
    reason: String,
    row: Option<u64>,
}

/// Active standalone-audio recording state.
///
/// Mirrors the video recorder's "begin/end with a path" shape but
/// without an external process — audio is teed into
/// [`HeadlessSession::audio_capture`] continuously, and the recording
/// slices that buffer at finish time. `start_offset` is the sample
/// count already in the capture when recording began.
#[derive(Debug)]
struct AudioRecording {
    path: PathBuf,
    start_offset: usize,
}

/// Summary returned by [`HeadlessSession::stop_audio_recording`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioRecordingSummary {
    /// Final WAV file path.
    pub path: PathBuf,
    /// Samples written (per channel, post-channel-interleave the file
    /// holds `samples * channels` 16-bit values).
    pub samples: usize,
    /// Sample rate of the captured stream, in Hz.
    pub sample_rate: u32,
    /// Channel count of the captured stream.
    pub channels: u8,
    /// Approximate clip duration in milliseconds (`samples * 1000 /
    /// sample_rate`, rounded down).
    pub duration_ms: u64,
}

/// Shared host-side session around one live machine runtime.
pub struct HeadlessSession<M, Q = NoAdditionalQueries> {
    machine: M,
    native_frame_ticks: u64,
    queued_input: Vec<InputEvent>,
    frame_capture: LatestFrameCapture,
    audio_capture: AudioCapture,
    trace_sink: NullTraceSink,
    last_run_result: Option<RunResult>,
    query_provider: Q,
    recorder: Option<VideoRecorder>,
    audio_offset_at_recording_start: usize,
    audio_recording: Option<AudioRecording>,
}

impl<M> HeadlessSession<M, NoAdditionalQueries> {
    /// Creates a new session around one live machine runtime.
    #[must_use]
    pub fn new(machine: M, native_frame_ticks: u64) -> Self {
        Self::new_with_query_provider(machine, native_frame_ticks, NoAdditionalQueries)
    }
}

impl<M, Q> HeadlessSession<M, Q> {
    /// Creates a new session around one live machine runtime with one
    /// additional family-owned query provider.
    #[must_use]
    pub fn new_with_query_provider(machine: M, native_frame_ticks: u64, query_provider: Q) -> Self {
        Self {
            machine,
            native_frame_ticks,
            queued_input: Vec::new(),
            frame_capture: LatestFrameCapture::default(),
            audio_capture: AudioCapture::default(),
            trace_sink: NullTraceSink,
            audio_recording: None,
            last_run_result: None,
            query_provider,
            recorder: None,
            audio_offset_at_recording_start: 0,
        }
    }

    /// Returns the wrapped machine runtime.
    #[must_use]
    pub fn machine(&self) -> &M {
        &self.machine
    }

    /// Returns mutable access to the wrapped machine runtime.
    #[must_use]
    pub fn machine_mut(&mut self) -> &mut M {
        &mut self.machine
    }

    /// Returns the configured native frame delta in machine ticks.
    #[must_use]
    pub const fn native_frame_ticks(&self) -> u64 {
        self.native_frame_ticks
    }

    /// Updates the native frame delta. Use after swapping the inner
    /// machine to a variant that paces at a different number of
    /// half-cycles per frame (e.g. Spectrum 128K's 70908 cycles vs
    /// 48K's 69888). Without this, `run_frames` would over- or
    /// under-shoot one native frame by a few hundred cycles on every
    /// call after the swap.
    pub const fn set_native_frame_ticks(&mut self, native_frame_ticks: u64) {
        self.native_frame_ticks = native_frame_ticks;
    }

    /// Consumes the session and returns the wrapped machine runtime.
    #[must_use]
    pub fn into_machine(self) -> M {
        self.machine
    }
}

impl<M: MachineCore, Q: SessionQueryProvider<M>> HeadlessSession<M, Q> {
    /// Returns the current authoritative machine time.
    #[must_use]
    pub fn time(&self) -> MachineTime {
        self.machine.time()
    }

    /// Returns the most recent run result, when one exists.
    #[must_use]
    pub const fn last_run_result(&self) -> Option<RunResult> {
        self.last_run_result
    }

    /// Returns the most recently captured video frame, when one has been emitted.
    #[must_use]
    pub fn latest_frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }

    /// Returns the supported shared query paths, optionally filtered by prefix.
    #[must_use]
    pub fn query_paths(&self, prefix: Option<&str>) -> QueryPathsResult {
        let mut result = query_paths(prefix);
        result
            .paths
            .extend(self.query_provider.query_paths(&self.machine, prefix));
        result.paths.sort_unstable();
        result.paths.dedup();
        result
    }

    /// Resolves one shared query path against the current session state.
    pub fn query(&self, path: &str) -> Result<QueryResult, QueryError> {
        match query_value(
            self.machine.profile(),
            self.time(),
            self.native_frame_ticks,
            self.frame_capture.frame().is_some(),
            self.audio_capture.audio().is_some(),
            self.last_run_result,
            path,
        ) {
            Ok(result) => Ok(result),
            Err(QueryError::UnknownPath { .. }) => self
                .query_provider
                .query(&self.machine, path)?
                .ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                }),
            Err(err) => Err(err),
        }
    }

    /// Runs native frames until the machine reports `boot.detected = true`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying machine run fails, if the runtime
    /// does not expose the generic `boot.*` query paths, if those paths resolve
    /// to unexpected value shapes, or if the frame budget expires before boot
    /// is detected.
    pub fn wait_for_boot(&mut self, max_frames: u32) -> Result<BootWaitResult, SessionError> {
        let mut state = self.boot_query_state()?;
        if state.detected {
            return Ok(BootWaitResult {
                frames: 0,
                reached: self.time(),
                reason: state.reason,
                row: state.row,
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames(1)?;
            state = self.boot_query_state()?;
            if state.detected {
                return Ok(BootWaitResult {
                    frames,
                    reached: result.reached,
                    reason: state.reason,
                    row: state.row,
                });
            }
        }

        Err(SessionError::BootTimeout {
            max_frames,
            reason: state.reason,
        })
    }

    /// Runs native frames until the machine reports `boot.detected = true`,
    /// emitting trace events to one caller-provided sink for this wait.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying machine run fails, if the runtime
    /// does not expose the generic `boot.*` query paths, if those paths resolve
    /// to unexpected value shapes, or if the frame budget expires before boot
    /// is detected.
    pub fn wait_for_boot_with_trace_sink(
        &mut self,
        max_frames: u32,
        trace_sink: &mut dyn TraceSink,
    ) -> Result<BootWaitResult, SessionError> {
        let mut state = self.boot_query_state()?;
        if state.detected {
            return Ok(BootWaitResult {
                frames: 0,
                reached: self.time(),
                reason: state.reason,
                row: state.row,
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames_with_trace_sink(1, trace_sink)?;
            state = self.boot_query_state()?;
            if state.detected {
                return Ok(BootWaitResult {
                    frames,
                    reached: result.reached,
                    reason: state.reason,
                    row: state.row,
                });
            }
        }

        Err(SessionError::BootTimeout {
            max_frames,
            reason: state.reason,
        })
    }

    /// Runs native frames until one text-bearing query contains one substring.
    ///
    /// The query value must be either one string or one array of strings.
    ///
    /// # Errors
    ///
    /// Returns an error if the query does not exist, if it resolves to a value
    /// that is not text-bearing, or if the frame budget expires before the
    /// substring is observed.
    pub fn wait_for_query_text_contains(
        &mut self,
        path: &str,
        needle: &str,
        max_frames: u32,
    ) -> Result<QueryTextWaitResult, SessionError> {
        if let Some((line, matched_text)) = self.query_text_contains(path, needle)? {
            return Ok(QueryTextWaitResult {
                path: path.to_owned(),
                needle: needle.to_owned(),
                frames: 0,
                reached: self.time(),
                line,
                matched_text,
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames(1)?;
            if let Some((line, matched_text)) = self.query_text_contains(path, needle)? {
                return Ok(QueryTextWaitResult {
                    path: path.to_owned(),
                    needle: needle.to_owned(),
                    frames,
                    reached: result.reached,
                    line,
                    matched_text,
                });
            }
        }

        Err(SessionError::QueryTextTimeout {
            path: path.to_owned(),
            needle: needle.to_owned(),
            max_frames,
        })
    }

    /// Runs native frames until one text-bearing query contains one substring,
    /// emitting trace events to one caller-provided sink for this wait.
    ///
    /// The query value must be either one string or one array of strings.
    ///
    /// # Errors
    ///
    /// Returns an error if the query does not exist, if it resolves to a value
    /// that is not text-bearing, or if the frame budget expires before the
    /// substring is observed.
    pub fn wait_for_query_text_contains_with_trace_sink(
        &mut self,
        path: &str,
        needle: &str,
        max_frames: u32,
        trace_sink: &mut dyn TraceSink,
    ) -> Result<QueryTextWaitResult, SessionError> {
        if let Some((line, matched_text)) = self.query_text_contains(path, needle)? {
            return Ok(QueryTextWaitResult {
                path: path.to_owned(),
                needle: needle.to_owned(),
                frames: 0,
                reached: self.time(),
                line,
                matched_text,
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames_with_trace_sink(1, trace_sink)?;
            if let Some((line, matched_text)) = self.query_text_contains(path, needle)? {
                return Ok(QueryTextWaitResult {
                    path: path.to_owned(),
                    needle: needle.to_owned(),
                    frames,
                    reached: result.reached,
                    line,
                    matched_text,
                });
            }
        }

        Err(SessionError::QueryTextTimeout {
            path: path.to_owned(),
            needle: needle.to_owned(),
            max_frames,
        })
    }

    /// Runs native frames until one boolean query path reaches one target
    /// value.
    ///
    /// # Errors
    ///
    /// Returns an error if the query does not exist, if it resolves to a value
    /// that is not boolean, or if the frame budget expires before the target
    /// value is observed.
    pub fn wait_for_query_bool(
        &mut self,
        path: &str,
        expected: bool,
        max_frames: u32,
    ) -> Result<QueryBoolWaitResult, SessionError> {
        if self.query_bool_at(path)? == expected {
            return Ok(QueryBoolWaitResult {
                path: path.to_owned(),
                expected,
                frames: 0,
                reached: self.time(),
            });
        }

        for frames in 1..=max_frames {
            let result = self.run_frames(1)?;
            if self.query_bool_at(path)? == expected {
                return Ok(QueryBoolWaitResult {
                    path: path.to_owned(),
                    expected,
                    frames,
                    reached: result.reached,
                });
            }
        }

        Err(SessionError::QueryBoolTimeout {
            path: path.to_owned(),
            expected,
            max_frames,
        })
    }

    /// Queues one input event for the next execution slice.
    pub fn queue_input(&mut self, event: InputEvent) {
        self.queued_input.push(event);
    }

    /// Queues multiple input events for the next execution slice.
    pub fn queue_inputs<I>(&mut self, events: I)
    where
        I: IntoIterator<Item = InputEvent>,
    {
        self.queued_input.extend(events);
    }

    /// Applies media inserts and control commands to the live machine.
    ///
    /// # Errors
    ///
    /// Returns an error if the target machine rejects the media or commands.
    pub fn prepare(
        &mut self,
        media: &MediaSet<'_>,
        commands: &[ControlCommand],
    ) -> Result<(), SessionError> {
        prepare_machine(&mut self.machine, media, commands)?;
        Ok(())
    }

    /// Applies one control command to the live machine.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine rejects the command.
    pub fn command(&mut self, command: &ControlCommand) -> Result<(), SessionError> {
        self.machine.command(command)?;
        Ok(())
    }

    /// Loads one media set into the live machine.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine rejects the media.
    pub fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), SessionError> {
        self.machine.load_media(media)?;
        Ok(())
    }

    /// Resets the underlying machine.
    ///
    /// Forwards to [`MachineCore::reset`]. Also clears session-side
    /// state that no longer makes sense across a reset (queued input,
    /// the latest captured frame, captured audio, the cached last
    /// run result).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::DisallowedDuringRecording`] when a
    /// video recording is in flight — a reset would jump-cut the
    /// clip, same rule as [`Self::restore_snapshot`].
    pub fn reset(&mut self, kind: ResetKind) -> Result<(), SessionError> {
        if self.recorder.is_some() {
            return Err(SessionError::DisallowedDuringRecording {
                operation: "reset",
            });
        }
        self.machine.reset(kind);
        self.queued_input.clear();
        self.frame_capture = LatestFrameCapture::default();
        self.audio_capture = AudioCapture::default();
        self.last_run_result = None;
        Ok(())
    }

    /// Restores one snapshot into the live machine.
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot restore fails, or
    /// [`SessionError::DisallowedDuringRecording`] when a video recording is
    /// in flight (a snapshot restore would jump-cut the clip).
    pub fn restore_snapshot(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        if self.recorder.is_some() {
            return Err(SessionError::DisallowedDuringRecording {
                operation: "restore_snapshot",
            });
        }
        self.machine.restore(bytes)?;
        Ok(())
    }

    /// Serializes the current machine snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot generation fails.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, SessionError> {
        Ok(self.machine.snapshot()?)
    }

    /// Writes the current machine snapshot to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if snapshot generation or file output fails.
    pub fn save_snapshot(&self, path: &Path) -> Result<(), SessionError> {
        std::fs::write(path, self.snapshot_bytes()?)?;
        Ok(())
    }

    /// Runs the machine until the requested target time.
    ///
    /// While a video recording is active, the call is internally chunked into
    /// native-frame-sized steps so every emitted frame can be teed into the
    /// recorder.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine or one host-side sink rejects the
    /// execution request, or if writing to the active recorder fails.
    pub fn run_until(&mut self, target: MachineTime) -> Result<RunResult, SessionError> {
        if self.recorder.is_some() {
            return self.run_until_recording(target);
        }
        self.run_until_inner(target)
    }

    fn run_until_inner(&mut self, target: MachineTime) -> Result<RunResult, SessionError> {
        let inputs = std::mem::take(&mut self.queued_input);
        let mut host = HostIo {
            input_events: &inputs,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_capture,
            trace_sink: &mut self.trace_sink,
        };
        let result = self.machine.run_until(target, &mut host)?;
        self.last_run_result = Some(result);
        Ok(result)
    }

    fn run_until_recording(&mut self, target: MachineTime) -> Result<RunResult, SessionError> {
        let mut last = RunResult::new(self.machine.time(), StopReason::ReachedTarget);
        while self.machine.time() < target {
            let chunk_end = self
                .machine
                .time()
                .saturating_add(self.native_frame_ticks)
                .get()
                .min(target.get());
            last = self.run_until_inner(MachineTime::new(chunk_end))?;
            self.tee_frame_to_recorder()?;
        }
        Ok(last)
    }

    fn tee_frame_to_recorder(&mut self) -> Result<(), SessionError> {
        let Self {
            frame_capture,
            recorder,
            ..
        } = self;
        if let (Some(recorder), Some(frame)) = (recorder.as_mut(), frame_capture.frame()) {
            recorder.push_frame(frame)?;
        }
        Ok(())
    }

    /// Runs the machine until the requested target time, emitting trace events
    /// to one caller-provided sink for this execution request.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine or one host-side sink rejects the
    /// execution request.
    pub fn run_until_with_trace_sink(
        &mut self,
        target: MachineTime,
        trace_sink: &mut dyn TraceSink,
    ) -> Result<RunResult, SessionError> {
        let inputs = std::mem::take(&mut self.queued_input);
        let mut host = HostIo {
            input_events: &inputs,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_capture,
            trace_sink,
        };
        let result = self.machine.run_until(target, &mut host)?;
        self.last_run_result = Some(result);
        Ok(result)
    }

    /// Runs the machine for `count` native video frames.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine or one host-side sink rejects the
    /// execution request.
    pub fn run_frames(&mut self, count: u32) -> Result<RunResult, SessionError> {
        let delta = self.native_frame_ticks.saturating_mul(u64::from(count));
        self.run_until(self.time().saturating_add(delta))
    }

    /// Runs the machine for `count` native video frames while emitting trace
    /// events to one caller-provided sink.
    ///
    /// # Errors
    ///
    /// Returns an error if the machine or one host-side sink rejects the
    /// execution request.
    pub fn run_frames_with_trace_sink(
        &mut self,
        count: u32,
        trace_sink: &mut dyn TraceSink,
    ) -> Result<RunResult, SessionError> {
        let delta = self.native_frame_ticks.saturating_mul(u64::from(count));
        self.run_until_with_trace_sink(self.time().saturating_add(delta), trace_sink)
    }

    /// Encodes the latest emitted frame as PNG.
    ///
    /// # Errors
    ///
    /// Returns an error if no frame has been emitted or if PNG encoding fails.
    pub fn screenshot_png_bytes(&self) -> Result<Vec<u8>, SessionError> {
        Ok(self.frame_capture.png_bytes()?)
    }

    /// Writes the latest emitted frame as PNG.
    ///
    /// # Errors
    ///
    /// Returns an error if no frame has been emitted or file output fails.
    pub fn save_screenshot(&self, path: &Path) -> Result<(), SessionError> {
        std::fs::write(path, self.screenshot_png_bytes()?)?;
        Ok(())
    }

    /// Encodes the accumulated audio stream as WAV.
    ///
    /// # Errors
    ///
    /// Returns an error if no audio has been emitted yet.
    pub fn audio_wav_bytes(&self) -> Result<Vec<u8>, SessionError> {
        Ok(self.audio_capture.wav_bytes()?)
    }

    /// Writes the accumulated audio stream as WAV.
    ///
    /// # Errors
    ///
    /// Returns an error if no audio has been emitted yet or file output fails.
    pub fn save_audio_capture(&self, path: &Path) -> Result<(), SessionError> {
        std::fs::write(path, self.audio_wav_bytes()?)?;
        Ok(())
    }

    /// Drops any captured audio accumulated so far.
    pub fn clear_audio_capture(&mut self) {
        self.audio_capture = AudioCapture::default();
    }

    /// Returns whether a video recording is currently in flight.
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Returns whether a standalone audio recording is currently in
    /// flight. Independent of `is_recording` (video) — both can be
    /// active concurrently and share the same underlying audio
    /// capture stream.
    #[must_use]
    pub const fn is_audio_recording(&self) -> bool {
        self.audio_recording.is_some()
    }

    /// Begins a standalone audio recording.
    ///
    /// Captures the current end-of-buffer position and remembers the
    /// output path. Subsequent `run_frames` / wait steps tee emitted
    /// audio into the session's audio buffer as usual; the WAV file
    /// is written when `stop_audio_recording` is called and contains
    /// only the samples emitted between the two calls. This mirrors
    /// the video recorder's start/stop ergonomics — no manual buffer
    /// management, no leaked silence at the head.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Audio`] with
    /// [`AudioRecordingError::AlreadyRecording`] if a standalone
    /// audio recording is already in flight. (A concurrent video
    /// recording is allowed — both can record from the same audio
    /// stream.)
    pub fn start_audio_recording(&mut self, output_path: PathBuf) -> Result<(), SessionError> {
        if self.audio_recording.is_some() {
            return Err(SessionError::Audio(AudioRecordingError::AlreadyRecording));
        }
        let start_offset = self.audio_capture.audio().map_or(0, |a| a.samples.len());
        self.audio_recording = Some(AudioRecording {
            path: output_path,
            start_offset,
        });
        Ok(())
    }

    /// Finalises the in-flight standalone audio recording.
    ///
    /// Slices the audio buffer from the offset captured at
    /// `start_audio_recording` to the current end, encodes it as a
    /// 16-bit PCM WAV, and writes it to disk.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Audio`] with
    /// [`AudioRecordingError::NotRecording`] when no audio recording
    /// is in flight, or [`AudioRecordingError::NoAudio`] when the
    /// recording window captured zero samples.
    pub fn stop_audio_recording(&mut self) -> Result<AudioRecordingSummary, SessionError> {
        let recording = self
            .audio_recording
            .take()
            .ok_or(SessionError::Audio(AudioRecordingError::NotRecording))?;
        let Some(captured) = self.audio_capture.audio() else {
            return Err(SessionError::Audio(AudioRecordingError::NoAudio));
        };
        let total = captured.samples.len();
        if total <= recording.start_offset {
            return Err(SessionError::Audio(AudioRecordingError::NoAudio));
        }
        let slice = &captured.samples[recording.start_offset..total];
        let per_channel_samples = slice.len() / usize::from(captured.channels.max(1));
        let sliced = crate::CapturedAudio {
            sample_rate: captured.sample_rate,
            channels: captured.channels,
            samples: slice.to_vec(),
        };
        let bytes = sliced.wav_bytes();
        std::fs::write(&recording.path, &bytes)?;
        let duration_ms = if captured.sample_rate > 0 {
            (per_channel_samples as u64 * 1000) / u64::from(captured.sample_rate)
        } else {
            0
        };
        Ok(AudioRecordingSummary {
            path: recording.path,
            samples: per_channel_samples,
            sample_rate: captured.sample_rate,
            channels: captured.channels,
            duration_ms,
        })
    }

    /// Begins a video recording, writing the final MP4 to `output_path`.
    ///
    /// Width and height are taken from the latest emitted frame, so callers
    /// must run at least one frame before invoking this. Frame rate is
    /// derived from the machine profile's clock and the session's native
    /// frame timing.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Video`] wrapping [`VideoRecordingError::AlreadyRecording`]
    /// when a recording is already in flight, [`VideoRecordingError::NoFrameYet`]
    /// when no frame has been emitted yet, or [`VideoRecordingError::FfmpegNotFound`]
    /// / [`VideoRecordingError::FfmpegSpawn`] when ffmpeg cannot start.
    pub fn start_video_recording(&mut self, output_path: PathBuf) -> Result<(), SessionError> {
        if self.recorder.is_some() {
            return Err(SessionError::Video(VideoRecordingError::AlreadyRecording));
        }
        let Some(frame) = self.frame_capture.frame() else {
            return Err(SessionError::Video(VideoRecordingError::NoFrameYet));
        };
        let fps = compute_fps(self.machine.profile().clock.rate, self.native_frame_ticks);
        let recorder = VideoRecorder::start(
            output_path,
            frame.width,
            frame.height,
            fps,
            self.machine.time(),
        )?;
        self.audio_offset_at_recording_start =
            self.audio_capture.audio().map_or(0, |a| a.samples.len());
        self.recorder = Some(recorder);
        Ok(())
    }

    /// Finalises the in-flight video recording.
    ///
    /// Audio captured between start and stop is muxed into the final MP4.
    ///
    /// # Errors
    ///
    /// Returns [`VideoRecordingError::NotRecording`] when no recording is in
    /// flight, or surfaces ffmpeg failures from the finalisation pass.
    pub fn stop_video_recording(&mut self) -> Result<VideoRecordingSummary, SessionError> {
        let recorder = self
            .recorder
            .take()
            .ok_or(SessionError::Video(VideoRecordingError::NotRecording))?;
        let trimmed = trim_audio_after(
            self.audio_capture.audio(),
            self.audio_offset_at_recording_start,
        );
        self.audio_offset_at_recording_start = 0;
        let summary = recorder.finish(trimmed.as_ref())?;
        Ok(summary)
    }

    fn boot_query_state(&self) -> Result<BootQueryState, SessionError> {
        let detected = self.query_bool("boot.detected")?;
        let reason = self
            .optional_query_string("boot.reason")
            .unwrap_or_else(|| "boot.detected remained false".to_owned());
        let row = self.optional_query_u64("boot.row");

        Ok(BootQueryState {
            detected,
            reason,
            row,
        })
    }

    fn query_bool(&self, path: &str) -> Result<bool, SessionError> {
        self.query_bool_at(path)
    }

    fn query_bool_at(&self, path: &str) -> Result<bool, SessionError> {
        let result = self.query(path)?;
        result
            .value
            .as_bool()
            .ok_or_else(|| SessionError::UnexpectedQueryValue {
                path: path.to_owned(),
                expected: "a boolean",
            })
    }

    fn optional_query_string(&self, path: &str) -> Option<String> {
        match self.query(path) {
            Ok(result) => result.value.as_str().map(str::to_owned),
            Err(QueryError::UnknownPath { .. } | QueryError::UnavailablePath { .. }) => None,
        }
    }

    fn optional_query_u64(&self, path: &str) -> Option<u64> {
        match self.query(path) {
            Ok(result) => result.value.as_u64(),
            Err(QueryError::UnknownPath { .. } | QueryError::UnavailablePath { .. }) => None,
        }
    }

    fn query_text_contains(
        &self,
        path: &str,
        needle: &str,
    ) -> Result<Option<(Option<u64>, String)>, SessionError> {
        let result = self.query(path)?;
        if let Some(text) = result.value.as_str() {
            return Ok(text.contains(needle).then(|| (None, text.to_owned())));
        }

        if let Some(lines) = result.value.as_array() {
            for (index, line) in lines.iter().enumerate() {
                let Some(text) = line.as_str() else {
                    return Err(SessionError::UnexpectedQueryValue {
                        path: path.to_owned(),
                        expected: "a string or array of strings",
                    });
                };
                if text.contains(needle) {
                    return Ok(Some((Some(index as u64), text.to_owned())));
                }
            }
            return Ok(None);
        }

        Err(SessionError::UnexpectedQueryValue {
            path: path.to_owned(),
            expected: "a string or array of strings",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::host::{AudioPacket, FramePacket, PixelFormat};
    use crate::machine::{
        Family, MachineId, MachineProfile, ProfileId, Region, ResetKind, StopReason, SupportTier,
    };
    use crate::media::{FirmwareRequirement, MediaSlot, WritebackPolicy};
    use crate::query::SessionQueryProvider;
    use crate::time::{ClockDesc, ClockRate};
    use crate::{MediaImage, MediaKind};
    use serde_json::json;

    const DUMMY_FRAME_PIXELS: [u8; 32 * 32] = [1; 32 * 32];

    struct DummyMachine {
        profile: MachineProfile,
        time: MachineTime,
        loaded_media: usize,
        commands: usize,
        restored: usize,
        received_inputs: Vec<InputEvent>,
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
                    clock: ClockDesc::new("master-cycle", ClockRate::from_hz(3_500_000)),
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
                loaded_media: 0,
                commands: 0,
                restored: 0,
                received_inputs: Vec::new(),
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

        fn reset(&mut self, _kind: ResetKind) {
            self.time = MachineTime::default();
        }

        fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
            self.loaded_media += media.images.len();
            Ok(())
        }

        fn run_until(
            &mut self,
            target: MachineTime,
            host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            self.received_inputs.extend_from_slice(host.input_events);
            self.time = target;

            host.frame_sink.push_frame(FramePacket {
                timestamp: target,
                format: PixelFormat::Indexed8,
                width: 32,
                height: 32,
                palette: Some(&[0x000000FF, 0xFFFFFFFF]),
                pixels: &DUMMY_FRAME_PIXELS,
            })?;

            host.audio_sink.push_audio(AudioPacket {
                timestamp: target,
                sample_rate: 44_100,
                channels: 1,
                samples: &[0.0, 0.5],
            })?;

            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }

        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![0x42, 0x43])
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
        fn query_paths(&self, _machine: &DummyMachine, prefix: Option<&str>) -> Vec<String> {
            [
                "boot.detected",
                "boot.reason",
                "boot.row",
                "dummy.time",
                "dummy.flag",
                "screen.text.lines",
            ]
            .into_iter()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect()
        }

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
                "dummy.time" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get()),
                })),
                "dummy.flag" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(machine.time.get() >= 2 * 69_888),
                })),
                "screen.text.lines" => Ok(Some(QueryResult {
                    path: path.to_owned(),
                    value: json!(if machine.time.get() >= 4 * 69_888 {
                        vec![
                            "READY".to_owned(),
                            "MANIC MINER".to_owned(),
                            "PRESS ENTER".to_owned(),
                        ]
                    } else {
                        vec!["READY".to_owned(), "LOADING".to_owned(), " ".to_owned()]
                    }),
                })),
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn session_runs_frames_and_captures_outputs() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let result = session.run_frames(2).expect("two frames should run");

        assert_eq!(result.reached, MachineTime::new(139_776));
        assert_eq!(session.time(), MachineTime::new(139_776));
        assert!(session.screenshot_png_bytes().is_ok());
        assert!(session.audio_wav_bytes().is_ok());
        assert_eq!(session.last_run_result(), Some(result));
    }

    #[test]
    fn session_prepares_media_and_commands() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &[0x00]));
        let commands = [ControlCommand::MediaTransport(
            crate::MediaTransportCommand::new("tape-1", crate::MediaTransportAction::Start),
        )];

        session
            .prepare(&media, &commands)
            .expect("session preparation should succeed");

        assert_eq!(session.machine().loaded_media, 1);
        assert_eq!(session.machine().commands, 1);
    }

    #[test]
    fn session_queues_inputs_for_next_run() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        session.queue_input(InputEvent::Key {
            name: "q".into(),
            pressed: true,
        });

        session.run_frames(1).expect("frame should run");
        assert_eq!(session.machine().received_inputs.len(), 1);

        session.run_frames(1).expect("second frame should run");
        assert_eq!(session.machine().received_inputs.len(), 1);
    }

    #[test]
    fn session_can_save_snapshot_and_capture_files() {
        let temp_dir = std::env::temp_dir();
        let snapshot_path = temp_dir.join(format!(
            "emu198x-shell-session-{}-state.pst",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-shell-session-{}-frame.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-shell-session-{}-audio.wav",
            std::process::id()
        ));
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);

        session.run_frames(1).expect("frame should run");
        session
            .save_snapshot(&snapshot_path)
            .expect("snapshot should be written");
        session
            .save_screenshot(&screenshot_path)
            .expect("screenshot should be written");
        session
            .save_audio_capture(&audio_path)
            .expect("wav should be written");

        assert!(snapshot_path.is_file());
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());

        let _ = std::fs::remove_file(snapshot_path);
        let _ = std::fs::remove_file(screenshot_path);
        let _ = std::fs::remove_file(audio_path);
    }

    #[test]
    fn session_clear_audio_capture_drops_previous_audio() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        session.run_frames(1).expect("frame should run");
        assert!(session.audio_wav_bytes().is_ok());

        session.clear_audio_capture();
        let result = session.audio_wav_bytes();
        assert!(matches!(
            result,
            Err(SessionError::Capture(CaptureError::MissingAudio))
        ));
    }

    #[test]
    fn session_can_query_shared_status_paths() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69888);
        session.run_frames(1).expect("frame should run");

        let time = session
            .query("session.time")
            .expect("time query should resolve");
        let capabilities = session
            .query("session.profile.capabilities")
            .expect("capability query should resolve");
        let paths = session.query_paths(Some("run.last."));

        assert_eq!(time.value, serde_json::json!(69888));
        assert_eq!(capabilities.value, serde_json::json!([]));
        assert_eq!(
            paths.paths,
            vec![
                "run.last.reached".to_owned(),
                "run.last.stop_reason".to_owned()
            ]
        );
    }

    #[test]
    fn session_query_provider_extends_shared_surface() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69888,
            DummyQueryProvider,
        );
        session.run_frames(1).expect("frame should run");

        let extra = session
            .query("dummy.time")
            .expect("provider query should resolve");
        let paths = session.query_paths(Some("dummy."));

        assert_eq!(extra.value, json!(69_888));
        assert_eq!(
            paths.paths,
            vec!["dummy.flag".to_owned(), "dummy.time".to_owned()]
        );
    }

    #[test]
    fn session_wait_for_boot_runs_until_detected() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let result = session
            .wait_for_boot(3)
            .expect("dummy boot should be detected on frame three");

        assert_eq!(
            result,
            BootWaitResult {
                frames: 3,
                reached: MachineTime::new(209_664),
                reason: "dummy boot banner is visible".to_owned(),
                row: Some(23),
            }
        );
        assert_eq!(session.time(), MachineTime::new(209_664));
    }

    #[test]
    fn session_wait_for_boot_times_out_with_last_reason() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let error = session
            .wait_for_boot(2)
            .expect_err("two frames should not reach dummy boot");

        assert!(matches!(
            error,
            SessionError::BootTimeout {
                max_frames: 2,
                reason
            } if reason == "dummy boot banner not visible yet"
        ));
        assert_eq!(session.time(), MachineTime::new(139_776));
    }

    #[test]
    fn session_wait_for_query_text_contains_runs_until_match() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let result = session
            .wait_for_query_text_contains("screen.text.lines", "MANIC MINER", 4)
            .expect("text match should be detected on frame four");

        assert_eq!(
            result,
            QueryTextWaitResult {
                path: "screen.text.lines".to_owned(),
                needle: "MANIC MINER".to_owned(),
                frames: 4,
                reached: MachineTime::new(279_552),
                line: Some(1),
                matched_text: "MANIC MINER".to_owned(),
            }
        );
    }

    #[test]
    fn session_wait_for_query_text_contains_times_out() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let error = session
            .wait_for_query_text_contains("screen.text.lines", "MANIC MINER", 3)
            .expect_err("three frames should not reach the title text");

        assert!(matches!(
            error,
            SessionError::QueryTextTimeout {
                ref path,
                ref needle,
                max_frames: 3
            } if path == "screen.text.lines" && needle == "MANIC MINER"
        ));
    }

    #[test]
    fn session_wait_for_query_bool_runs_until_match() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let result = session
            .wait_for_query_bool("dummy.flag", true, 2)
            .expect("boolean query should become true on frame two");

        assert_eq!(
            result,
            QueryBoolWaitResult {
                path: "dummy.flag".to_owned(),
                expected: true,
                frames: 2,
                reached: MachineTime::new(139_776),
            }
        );
    }

    #[test]
    fn start_video_recording_rejects_when_no_frame_has_been_emitted() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        let path = std::env::temp_dir().join(format!(
            "emu198x-session-no-frame-{}.mp4",
            std::process::id()
        ));
        let err = session
            .start_video_recording(path)
            .expect_err("should refuse pre-frame start");
        assert!(matches!(
            err,
            SessionError::Video(crate::video::VideoRecordingError::NoFrameYet)
        ));
        assert!(!session.is_recording());
    }

    #[test]
    fn restore_snapshot_is_blocked_while_recording_is_active() {
        if crate::video::find_ffmpeg().is_none() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        session.run_frames(1).expect("frame should run");

        let path = std::env::temp_dir().join(format!(
            "emu198x-session-guard-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        session
            .start_video_recording(path.clone())
            .expect("recording should start");
        assert!(session.is_recording());

        let err = session
            .restore_snapshot(&[0x00])
            .expect_err("snapshot restore should be blocked");
        assert!(matches!(
            err,
            SessionError::DisallowedDuringRecording {
                operation: "restore_snapshot",
            }
        ));

        let summary = session
            .stop_video_recording()
            .expect("recording should stop cleanly");
        assert!(!session.is_recording());
        let _ = std::fs::remove_file(summary.path);
    }

    #[test]
    fn nested_start_video_recording_is_rejected() {
        if crate::video::find_ffmpeg().is_none() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        session.run_frames(1).expect("frame should run");

        let path = std::env::temp_dir().join(format!(
            "emu198x-session-nested-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        session
            .start_video_recording(path.clone())
            .expect("first start should succeed");

        let err = session
            .start_video_recording(path.clone())
            .expect_err("second start should be rejected");
        assert!(matches!(
            err,
            SessionError::Video(crate::video::VideoRecordingError::AlreadyRecording)
        ));

        let summary = session.stop_video_recording().expect("stop should succeed");
        let _ = std::fs::remove_file(summary.path);
    }

    #[test]
    fn stop_video_recording_without_start_errors() {
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        let err = session
            .stop_video_recording()
            .expect_err("stop without start should error");
        assert!(matches!(
            err,
            SessionError::Video(crate::video::VideoRecordingError::NotRecording)
        ));
    }

    #[test]
    fn pre_recording_audio_is_trimmed_so_tape_loader_does_not_leak() {
        if crate::video::find_ffmpeg().is_none() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        // Three warmup frames before recording — DummyMachine emits two
        // audio samples per frame, so the capture buffer holds six
        // samples by the time recording starts.
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        for _ in 0..3 {
            session.run_frames(1).expect("warmup frame should run");
        }
        let pre_recording_samples = session
            .audio_capture
            .audio()
            .expect("audio captured during warmup")
            .samples
            .len();
        assert_eq!(pre_recording_samples, 6);

        let path = std::env::temp_dir().join(format!(
            "emu198x-session-trim-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        session
            .start_video_recording(path.clone())
            .expect("start should succeed");
        // The session must remember where audio capture stood at the
        // moment recording began, so that stop can trim everything that
        // came before — which on a real Spectrum boot includes the
        // tape-loader audio.
        assert_eq!(
            session.audio_offset_at_recording_start,
            pre_recording_samples
        );

        session
            .run_frames(5)
            .expect("five frames should run while recording");

        let summary = session.stop_video_recording().expect("stop should succeed");
        assert_eq!(summary.frames, 5);
        assert!(summary.has_audio);
        // Offset resets after stop so a second recording does not
        // re-trim against a stale mark.
        assert_eq!(session.audio_offset_at_recording_start, 0);
        let _ = std::fs::remove_file(summary.path);
    }

    #[test]
    fn run_frames_during_recording_tees_each_frame_into_the_recorder() {
        if crate::video::find_ffmpeg().is_none() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let mut session = HeadlessSession::new(DummyMachine::new(), 69_888);
        session.run_frames(1).expect("warmup frame");

        let path = std::env::temp_dir().join(format!(
            "emu198x-session-tee-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        session
            .start_video_recording(path.clone())
            .expect("start should succeed");
        session
            .run_frames(10)
            .expect("ten frames should run while recording");

        let summary = session.stop_video_recording().expect("stop should succeed");
        assert_eq!(summary.frames, 10);
        assert!(summary.path.is_file());
        assert!(std::fs::metadata(&summary.path).expect("metadata").len() > 0);
        let _ = std::fs::remove_file(summary.path);
    }

    #[test]
    fn session_wait_for_query_bool_times_out() {
        let mut session = HeadlessSession::new_with_query_provider(
            DummyMachine::new(),
            69_888,
            DummyQueryProvider,
        );

        let error = session
            .wait_for_query_bool("dummy.flag", true, 1)
            .expect_err("one frame should not reach the boolean query target");

        assert!(matches!(
            error,
            SessionError::QueryBoolTimeout {
                ref path,
                expected: true,
                max_frames: 1
            } if path == "dummy.flag"
        ));
    }
}
