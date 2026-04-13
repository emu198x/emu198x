//! Sinclair ZX Spectrum family metadata.
//!
//! This crate owns the Spectrum family's metadata catalogue plus the first
//! runtime wrapper over the 48K machine implementation. The wrapper is the
//! shared-control-surface boundary: it translates `MediaSet`, host input
//! events, and frame/audio sinks into concrete 48K machine operations.

use emu198x_shell::{
    CapabilitySet, ClockDesc, ClockRate, Family, FirmwareRequirement, MachineId, MachineProfile,
    MediaKind, MediaSlot, ProfileId, Region, SupportTier, WritebackPolicy, known_capability,
};

mod runtime;

pub use runtime::{Spectrum48kRuntime, SpectrumSessionQueryProvider};

/// Supported Spectrum family models in the initial bootstrap pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    /// ZX Spectrum 48K PAL.
    Spectrum48KPal,
    /// ZX Spectrum 128K PAL.
    Spectrum128KPal,
}

impl Model {
    /// Stable model identifier for this model.
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k",
        }
    }

    /// Stable profile identifier for this model.
    #[must_use]
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "sinclair-zx-spectrum-48k-pal",
            Self::Spectrum128KPal => "sinclair-zx-spectrum-128k-pal",
        }
    }

    /// User-facing display name for this model.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Spectrum48KPal => "ZX Spectrum 48K (PAL)",
            Self::Spectrum128KPal => "ZX Spectrum 128K (PAL)",
        }
    }
}

/// Returns the initial Spectrum family catalogue.
#[must_use]
pub fn profiles() -> Vec<MachineProfile> {
    vec![
        profile_for(Model::Spectrum48KPal),
        profile_for(Model::Spectrum128KPal),
    ]
}

/// Returns the profile metadata for one Spectrum model.
#[must_use]
pub fn profile_for(model: Model) -> MachineProfile {
    match model {
        Model::Spectrum48KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: 1982,
            summary: "48K PAL baseline for the first reference Spectrum implementation.".into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(14_000_000)),
            firmware: vec![FirmwareRequirement::new(
                "sinclair-zx-spectrum-48k-rom",
                "ZX Spectrum 48K ROM",
                false,
            )],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::with_all([
                known_capability("beeper-audio"),
                known_capability("keyboard-matrix"),
                known_capability("snapshot-export"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        },
        Model::Spectrum128KPal => MachineProfile {
            machine_id: MachineId::from("sinclair-zx-spectrum"),
            profile_id: ProfileId::from(model.profile_id()),
            display_name: model.display_name().into(),
            family: Family::Spectrum,
            region: Region::Pal,
            support_tier: SupportTier::Research,
            release_year: 1985,
            summary:
                "128K PAL follow-on profile with banked memory, AY audio, and tape-era baseline media."
                    .into(),
            clock: ClockDesc::new("master-cycle", ClockRate::from_hz(17_734_475)),
            firmware: vec![
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-0",
                    "ZX Spectrum 128K ROM 0",
                    false,
                ),
                FirmwareRequirement::new(
                    "sinclair-zx-spectrum-128k-rom-1",
                    "ZX Spectrum 128K ROM 1",
                    false,
                ),
            ],
            media_slots: vec![MediaSlot::new(
                "tape-1",
                "Tape Deck",
                MediaKind::Tape,
                false,
                WritebackPolicy::InMemoryOnly,
            )],
            capabilities: CapabilitySet::with_all([
                known_capability("ay-audio"),
                known_capability("banked-memory"),
                known_capability("keyboard-matrix"),
                known_capability("tape-input"),
                known_capability("tape-transport-control"),
                known_capability("snapshot-import"),
                known_capability("scripted-input"),
            ]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::MemoryBus;
    use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
    use emu198x_shell::{
        AudioPacket, AudioSink, ControlCommand, FirmwareImage, FirmwareSet, FramePacket, FrameSink,
        HeadlessSession, HostIo, InputEvent, MachineCore, MachineError, MachineTime, MediaImage,
        MediaSet, MediaTransportAction, MediaTransportCommand, NullTraceSink, PixelFormat,
        SessionQueryProvider,
    };
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn profile_ids_are_unique() {
        let profiles = profiles();
        let mut ids: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn spectrum_48k_uses_documented_master_clock() {
        let profile = profile_for(Model::Spectrum48KPal);
        assert_eq!(profile.clock.unit.as_ref(), "master-cycle");
        assert_eq!(profile.clock.rate.numerator_hz, 14_000_000);
        assert_eq!(profile.clock.rate.denominator_hz, 1);
    }

    #[test]
    fn all_profiles_require_firmware() {
        for profile in profiles() {
            assert!(
                !profile.firmware.is_empty(),
                "{} should declare firmware",
                profile.display_name
            );
        }
    }

    #[test]
    fn runtime_loads_tap_media_into_tape_slot() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let tap = minimal_tap();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));

        runtime
            .load_media(&media)
            .expect("valid TAP should load into runtime");

        assert!(runtime.machine().tape_is_loaded());
        assert!(!runtime.machine().tape_is_playing());
    }

    #[test]
    fn runtime_can_boot_from_declared_firmware_set() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &[0; 16 * 1024],
        ));

        let runtime = Spectrum48kRuntime::from_firmware(&firmware);
        assert!(
            runtime.is_ok(),
            "declared 48K firmware should boot the runtime"
        );
    }

    #[test]
    fn runtime_rejects_unknown_media_slot() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let tap = minimal_tap();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-9", MediaKind::Tape, &tap));

        let err = runtime
            .load_media(&media)
            .expect_err("unknown slot must be rejected");
        assert!(matches!(
            err,
            MachineError::UnknownMediaSlot { ref slot } if slot == "tape-9"
        ));
    }

    #[test]
    fn runtime_run_until_emits_frame_and_audio_packets() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let mut frame_sink = RecordingFrameSink::default();
        let mut audio_sink = RecordingAudioSink::default();
        let mut trace_sink = NullTraceSink;
        let inputs = [InputEvent::Key {
            name: "q".into(),
            pressed: true,
        }];
        let target = MachineTime::new(u64::from(TIMING_48K.halfcycles_per_frame));
        let mut host = HostIo {
            input_events: &inputs,
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };

        let result = runtime
            .run_until(target, &mut host)
            .expect("one frame should run");

        assert_eq!(result.reached, target);
        assert_eq!(frame_sink.frames, 1);
        assert_eq!(
            frame_sink.last_dimensions,
            Some((SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32))
        );
        assert_eq!(frame_sink.last_format, Some(PixelFormat::Indexed8));
        assert_eq!(audio_sink.packets, 1);
        assert!(audio_sink.last_samples > 0);
        assert_eq!(runtime.machine().read_fe(0xfbfe) & 0x01, 0x00);
    }

    #[test]
    fn runtime_command_controls_tape_transport() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let tap = minimal_tap();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));
        runtime
            .load_media(&media)
            .expect("valid TAP should load before transport commands");

        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("tape start command should succeed");
        assert!(runtime.machine().tape_is_playing());

        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Stop,
            )))
            .expect("tape stop command should succeed");
        assert!(!runtime.machine().tape_is_playing());
    }

    #[test]
    fn runtime_snapshot_roundtrips_machine_state() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let tap = minimal_tap();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));
        runtime
            .load_media(&media)
            .expect("valid TAP should load before snapshot");
        runtime.machine_mut().write(0x8000, 0x42);
        runtime.machine_mut().apply_input_event(&InputEvent::Key {
            name: "q".into(),
            pressed: true,
        });
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("tape start command should succeed");

        let mut frame_sink = RecordingFrameSink::default();
        let mut audio_sink = RecordingAudioSink::default();
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(
                MachineTime::new(u64::from(TIMING_48K.halfcycles_per_frame)),
                &mut host,
            )
            .expect("runtime should advance before snapshot");

        let snapshot = runtime.snapshot().expect("snapshot should encode");
        let before_restore = snapshot.clone();
        let mut restored = Spectrum48kRuntime::blank();
        restored
            .restore(&snapshot)
            .expect("snapshot should restore into blank runtime");
        let after_restore = restored
            .snapshot()
            .expect("restored snapshot should encode");

        assert_eq!(after_restore, before_restore);
    }

    #[test]
    fn spectrum_query_provider_lists_supported_paths() {
        let runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let provider = SpectrumSessionQueryProvider;

        let paths = provider.query_paths(&runtime, Some("spectrum.tape."));

        assert_eq!(
            paths,
            vec![
                "spectrum.tape.loaded".to_owned(),
                "spectrum.tape.playing".to_owned()
            ]
        );
    }

    #[test]
    fn spectrum_query_provider_lists_boot_and_screen_text_paths() {
        let runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let provider = SpectrumSessionQueryProvider;

        let boot_paths = provider.query_paths(&runtime, Some("boot."));
        let screen_paths = provider.query_paths(&runtime, Some("screen.text."));

        assert_eq!(
            boot_paths,
            vec![
                "boot.detected".to_owned(),
                "boot.reason".to_owned(),
                "boot.row".to_owned()
            ]
        );
        assert_eq!(
            screen_paths,
            vec![
                "screen.text.cols".to_owned(),
                "screen.text.lines".to_owned(),
                "screen.text.rows".to_owned()
            ]
        );
    }

    #[test]
    fn spectrum_query_provider_reads_runtime_state() {
        let mut runtime =
            Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024]).expect("dummy ROM should load");
        let tap = minimal_tap();
        let mut media = MediaSet::new();
        media.push(MediaImage::new("tape-1", MediaKind::Tape, &tap));
        runtime
            .load_media(&media)
            .expect("valid TAP should load before query");
        runtime
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                "tape-1",
                MediaTransportAction::Start,
            )))
            .expect("tape start command should succeed");
        runtime.machine_mut().advance_tstates(9);
        let provider = SpectrumSessionQueryProvider;

        let issue = provider
            .query(&runtime, "spectrum.machine.issue")
            .expect("issue query should resolve")
            .expect("provider should own issue path");
        let boot = provider
            .query(&runtime, "boot.detected")
            .expect("boot query should resolve")
            .expect("provider should own boot path");
        let cols = provider
            .query(&runtime, "screen.text.cols")
            .expect("screen text cols query should resolve")
            .expect("provider should own screen text cols path");
        let lines = provider
            .query(&runtime, "screen.text.lines")
            .expect("screen text lines query should resolve")
            .expect("provider should own screen text lines path");
        let tstate = provider
            .query(&runtime, "spectrum.machine.tstate_in_frame")
            .expect("tstate query should resolve")
            .expect("provider should own tstate path");
        let tape = provider
            .query(&runtime, "spectrum.tape.playing")
            .expect("tape query should resolve")
            .expect("provider should own tape path");

        assert_eq!(issue.value, serde_json::json!("issue3"));
        assert_eq!(boot.value, serde_json::json!(false));
        assert_eq!(cols.value, serde_json::json!(32));
        assert_eq!(
            lines
                .value
                .as_array()
                .expect("screen text lines should be returned as a JSON array")
                .len(),
            24
        );
        assert_eq!(tstate.value, serde_json::json!(9));
        assert_eq!(tape.value, serde_json::json!(true));
    }

    #[test]
    #[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
    fn spectrum_query_provider_detects_booted_48k_rom() {
        let Some(rom_path) = spectrum_48k_rom_path() else {
            eprintln!("HOME is not set; skipping ROM-backed Spectrum boot detection test");
            return;
        };

        if !rom_path.is_file() {
            eprintln!("ROM not found at {}", rom_path.display());
            return;
        }

        let rom = match fs::read(&rom_path) {
            Ok(rom) => rom,
            Err(err) => panic!("failed to read {}: {err}", rom_path.display()),
        };

        let mut runtime = Spectrum48kRuntime::from_rom_bytes(&rom)
            .expect("48K ROM path should contain a valid 16 KiB image");
        let mut frame_sink = RecordingFrameSink::default();
        let mut audio_sink = RecordingAudioSink::default();
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        let provider = SpectrumSessionQueryProvider;

        runtime
            .run_until(
                MachineTime::new(u64::from(TIMING_48K.halfcycles_per_frame) * 200),
                &mut host,
            )
            .expect("48K runtime should reach the copyright screen");

        let detected = provider
            .query(&runtime, "boot.detected")
            .expect("boot detection query should resolve")
            .expect("provider should own boot detection path");
        let reason = provider
            .query(&runtime, "boot.reason")
            .expect("boot reason query should resolve")
            .expect("provider should own boot reason path");
        let row = provider
            .query(&runtime, "boot.row")
            .expect("boot row query should resolve")
            .expect("provider should own boot row path");
        let lines = provider
            .query(&runtime, "screen.text.lines")
            .expect("screen text lines query should resolve")
            .expect("provider should own screen text lines path");
        let line_values = lines
            .value
            .as_array()
            .expect("screen text lines should be returned as a JSON array");
        let detected_row =
            row.value
                .as_u64()
                .expect("boot row should resolve to one decoded text row") as usize;

        assert_eq!(detected.value, serde_json::json!(true));
        assert_eq!(
            reason.value,
            serde_json::json!(format!("found copyright banner on row {detected_row}"))
        );
        assert!(detected_row < line_values.len());
        assert!(
            line_values[detected_row]
                .as_str()
                .is_some_and(|line| line.contains("© 1982 Sinclair Research Ltd")),
            "decoded screen text should contain the 48K copyright banner"
        );
    }

    #[test]
    #[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
    fn spectrum_boot_wait_and_prompt_input_change_decoded_text() {
        let Some(rom_path) = spectrum_48k_rom_path() else {
            eprintln!("HOME is not set; skipping ROM-backed Spectrum prompt input test");
            return;
        };

        if !rom_path.is_file() {
            eprintln!("ROM not found at {}", rom_path.display());
            return;
        }

        let rom = match fs::read(&rom_path) {
            Ok(rom) => rom,
            Err(err) => panic!("failed to read {}: {err}", rom_path.display()),
        };

        let runtime = Spectrum48kRuntime::from_rom_bytes(&rom)
            .expect("48K ROM path should contain a valid 16 KiB image");
        let mut session = HeadlessSession::new_with_query_provider(
            runtime,
            u64::from(TIMING_48K.halfcycles_per_frame),
            SpectrumSessionQueryProvider,
        );

        let boot = session
            .wait_for_boot(250)
            .expect("48K ROM should reach boot detection within 250 frames");
        assert_eq!(boot.row, Some(23));

        session.queue_input(InputEvent::Key {
            name: "enter".into(),
            pressed: true,
        });
        session
            .run_frames(2)
            .expect("prompt exposure should advance with Enter held");
        session.queue_input(InputEvent::Key {
            name: "enter".into(),
            pressed: false,
        });
        session
            .run_frames(2)
            .expect("prompt exposure should advance after Enter release");

        let prompt_lines = screen_text_lines_from_session(&session);
        assert_eq!(prompt_lines[23].trim_end(), "K");

        session.queue_input(InputEvent::Key {
            name: "a".into(),
            pressed: true,
        });
        session
            .run_frames(2)
            .expect("keyword entry should advance with A held");
        session.queue_input(InputEvent::Key {
            name: "a".into(),
            pressed: false,
        });
        session
            .run_frames(2)
            .expect("keyword entry should advance after A release");

        let edited_lines = screen_text_lines_from_session(&session);
        assert!(
            edited_lines[23].starts_with("NEW"),
            "expected BASIC prompt input to begin entering NEW, got {:?}",
            edited_lines[23]
        );
    }

    fn minimal_tap() -> Vec<u8> {
        let mut tap = vec![0x13, 0x00];
        tap.push(0x00);
        tap.extend_from_slice(&[0; 17]);
        tap.push(0x00);
        tap
    }

    fn spectrum_48k_rom_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
    }

    fn screen_text_lines_from_session(
        session: &HeadlessSession<Spectrum48kRuntime, SpectrumSessionQueryProvider>,
    ) -> Vec<String> {
        let result = session
            .query("screen.text.lines")
            .expect("screen text lines query should resolve");
        result
            .value
            .as_array()
            .expect("screen text lines query should return a JSON array")
            .iter()
            .map(|line| {
                line.as_str()
                    .expect("screen text lines should contain strings")
                    .to_owned()
            })
            .collect()
    }

    #[derive(Default)]
    struct RecordingFrameSink {
        frames: usize,
        last_dimensions: Option<(u32, u32)>,
        last_format: Option<PixelFormat>,
    }

    impl FrameSink for RecordingFrameSink {
        fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
            self.frames += 1;
            self.last_dimensions = Some((frame.width, frame.height));
            self.last_format = Some(frame.format);
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingAudioSink {
        packets: usize,
        last_samples: usize,
    }

    impl AudioSink for RecordingAudioSink {
        fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
            self.packets += 1;
            self.last_samples = packet.samples.len();
            Ok(())
        }
    }
}
