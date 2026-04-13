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
        HostIo, InputEvent, MachineCore, MachineError, MachineTime, MediaImage, MediaSet,
        MediaTransportAction, MediaTransportCommand, NullTraceSink, PixelFormat,
        SessionQueryProvider,
    };

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
        let tstate = provider
            .query(&runtime, "spectrum.machine.tstate_in_frame")
            .expect("tstate query should resolve")
            .expect("provider should own tstate path");
        let tape = provider
            .query(&runtime, "spectrum.tape.playing")
            .expect("tape query should resolve")
            .expect("provider should own tape path");

        assert_eq!(issue.value, serde_json::json!("issue3"));
        assert_eq!(tstate.value, serde_json::json!(9));
        assert_eq!(tape.value, serde_json::json!(true));
    }

    fn minimal_tap() -> Vec<u8> {
        let mut tap = vec![0x13, 0x00];
        tap.push(0x00);
        tap.extend_from_slice(&[0; 17]);
        tap.push(0x00);
        tap
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
