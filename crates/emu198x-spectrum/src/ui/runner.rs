//! Host-side runtime wrapper for UI mode.
//!
//! `SpectrumRunner` owns the live runtime (via `Box<dyn LiveSpectrumRuntime>`)
//! plus the host-side capture/audio sinks the UI's frame pacing reads.
//! It hides the `LiveSpectrumRuntime` trait behind a smaller, ui-flavoured
//! surface (commands, frame pacing, audio toggles, queries, window
//! title), giving the App layer one cohesive object to drive.

use std::time::Duration;

use emu198x_shell::query::query_value;
use emu198x_shell::{
    CapturedFrame, ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession, HostIo,
    InputEvent, LatestFrameCapture, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand, NativeAudioOutput, NullTraceSink, QueryError, QueryResult, ResetKind,
    RunResult, read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    AudioControls, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, DEFAULT_TAPE_AUTOLOAD_SLOT, SpeakerChannel,
    Spectrum48kRuntime, SpectrumSessionQueryProvider, autoload_basic_tape,
};

use crate::AppError;
use crate::live_machine::LiveSpectrumRuntime;
use crate::ui::Cli;
use crate::ui::input::next_audio_gain;

pub(crate) const DEFAULT_ROM_ID: &str = "sinclair-zx-spectrum-48k-rom";
pub(crate) const DEFAULT_TAPE_SLOT: &str = "tape-1";
const MAX_AUDIO_BUFFER_MS: u32 = 250;

pub struct SpectrumRunner {
    runtime: Box<dyn LiveSpectrumRuntime>,
    frame_capture: LatestFrameCapture,
    audio_output: NativeAudioOutput,
    last_run_result: Option<RunResult>,
    pub(crate) native_frame_ticks: u64,
}

impl SpectrumRunner {
    pub fn from_cli(cli: &Cli) -> Result<Self, AppError> {
        if cli.play_tape && cli.autoload_tape {
            return Err(AppError::ConflictingTapeWorkflow);
        }

        let rom_path = resolve_rom_path(cli)?;
        let rom = read_firmware_asset(&rom_path)?.bytes;

        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(DEFAULT_ROM_ID, &rom));
        let runtime = Spectrum48kRuntime::from_firmware(&firmware)?;
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(common_sinclair_zx_spectrum::timing::TIMING_48K.halfcycles_per_frame),
            SpectrumSessionQueryProvider,
        );

        if let Some(tape_path) = &cli.tape {
            let tape = read_media_asset(tape_path, MediaKind::Tape)?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new(
                DEFAULT_TAPE_SLOT,
                MediaKind::Tape,
                &tape.bytes,
            ));
            session.load_media(&media)?;
        }

        if cli.autoload_tape {
            if cli.tape.is_none() {
                return Err(AppError::MissingTape);
            }
            autoload_basic_tape(
                &mut session,
                DEFAULT_TAPE_AUTOLOAD_SLOT,
                DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            )?;
        } else if cli.play_tape {
            if cli.tape.is_none() {
                return Err(AppError::MissingTape);
            }
            session.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                DEFAULT_TAPE_SLOT,
                MediaTransportAction::Start,
            )))?;
        }

        let runtime: Box<dyn LiveSpectrumRuntime> = Box::new(session.into_machine());
        let native_frame_ticks = u64::from(runtime.frame_halfcycles());
        let audio_output = NativeAudioOutput::new(MAX_AUDIO_BUFFER_MS)?;
        let mut runner = Self {
            runtime,
            frame_capture: LatestFrameCapture::default(),
            audio_output,
            last_run_result: None,
            native_frame_ticks,
        };
        runner.run_frame(&[])?;
        Ok(runner)
    }

    /// Replaces the live runtime in-place. Drops the previous boxed
    /// runtime, swaps in the new one, recomputes the per-variant frame
    /// length, and clears host-side state so frame timestamps and
    /// audio continuity start fresh.
    pub fn replace_runtime(&mut self, runtime: Box<dyn LiveSpectrumRuntime>) {
        self.runtime = runtime;
        self.native_frame_ticks = u64::from(self.runtime.frame_halfcycles());
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.last_run_result = None;
    }

    pub fn reset(&mut self) -> Result<(), AppError> {
        self.runtime.reset(ResetKind::Hard);
        self.last_run_result = None;
        self.frame_capture = LatestFrameCapture::default();
        self.audio_output.clear();
        self.run_frame(&[])?;
        Ok(())
    }

    pub fn command(&mut self, command: &ControlCommand) -> Result<(), AppError> {
        self.runtime.command(command)?;
        Ok(())
    }

    pub fn run_frame(&mut self, input_events: &[InputEvent]) -> Result<(), AppError> {
        let _ = self.run_ticks(input_events, self.native_frame_ticks)?;
        Ok(())
    }

    pub fn run_ticks(
        &mut self,
        input_events: &[InputEvent],
        ticks: u64,
    ) -> Result<bool, AppError> {
        let previous_frame_timestamp = self.frame().map(|frame| frame.timestamp);
        let target = self.runtime.time().saturating_add(ticks);
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events,
            frame_sink: &mut self.frame_capture,
            audio_sink: &mut self.audio_output,
            trace_sink: &mut trace_sink,
        };
        self.last_run_result = Some(self.runtime.run_until(target, &mut host)?);
        Ok(self.frame().map(|frame| frame.timestamp) != previous_frame_timestamp)
    }

    pub fn frame_duration(&self) -> Duration {
        self.runtime.frame_duration()
    }

    pub fn frame(&self) -> Option<&CapturedFrame> {
        self.frame_capture.frame()
    }

    pub fn toggle_audio_channel(&mut self, channel: SpeakerChannel) -> bool {
        let controls = self.runtime.audio_controls();
        let enabled = !controls.channel(channel).enabled();
        self.runtime.set_audio_channel_enabled(channel, enabled);
        enabled
    }

    pub fn cycle_audio_channel_gain(&mut self, channel: SpeakerChannel) -> f32 {
        let controls = self.runtime.audio_controls();
        let next = next_audio_gain(controls.channel(channel).gain());
        self.runtime.set_audio_channel_gain(channel, next);
        next
    }

    pub fn reset_audio_controls(&mut self) {
        self.runtime.set_audio_controls(AudioControls::default());
    }

    pub fn query(&self, path: &str) -> Result<QueryResult, AppError> {
        match query_value(
            self.runtime.profile(),
            self.runtime.time(),
            self.native_frame_ticks,
            self.frame().is_some(),
            false,
            self.last_run_result,
            path,
        ) {
            Ok(result) => Ok(result),
            Err(QueryError::UnknownPath { .. }) => self
                .runtime
                .query(path)?
                .ok_or_else(|| QueryError::UnknownPath {
                    path: path.to_owned(),
                })
                .map_err(AppError::from),
            Err(err) => Err(AppError::from(err)),
        }
    }

    pub fn query_bool(&self, path: &str) -> bool {
        self.query(path)
            .ok()
            .and_then(|result| result.value.as_bool())
            .unwrap_or(false)
    }

    pub fn window_title(&self) -> String {
        // Kept deliberately cheap so the per-frame title update doesn't
        // walk the screen-text grid. Tape state is two flag reads; the
        // boot-banner / row-23-prompt decoration that used to live here
        // ran a full 24×32 cell decode against 96 ROM glyphs twice per
        // frame and dominated the GUI's frame budget. Variant identity
        // comes from the live runtime's profile.display_name — updates
        // on Machine-menu switch.
        let tape = match (
            self.query_bool("spectrum.tape.loaded"),
            self.query_bool("spectrum.tape.playing"),
        ) {
            (true, true) => "tape playing",
            (true, false) => "tape loaded",
            (false, _) => "no tape",
        };
        let display_name = self.runtime.profile().display_name.as_ref();
        format!("Emu198x | {display_name} | {tape}")
    }

    pub fn tape_playing(&self) -> bool {
        self.query_bool("spectrum.tape.playing")
    }
}

fn resolve_rom_path(cli: &Cli) -> Result<std::path::PathBuf, AppError> {
    if let Some(path) = &cli.rom {
        return Ok(path.clone());
    }

    let default = default_rom_path();
    if default.is_file() {
        Ok(default)
    } else {
        Err(AppError::MissingRom {
            path: default.display().to_string(),
        })
    }
}

fn default_rom_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
}
