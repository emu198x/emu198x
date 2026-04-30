//! C64 runtime construction, audio controls, drive attachment, and
//! the basic synthetic-media load path.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    AudioPacket, AudioSink, ControlCommand, FirmwareImage, FirmwareSet, HostIo, InputEvent,
    MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, MediaTransportAction,
    MediaTransportCommand, NullAudioSink, NullTraceSink, PixelFormat, ResetKind,
    SessionQueryProvider, StopReason, TraceEvent, TraceSink,
};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model, SidChannel, file_loader};
use serde_json::json;

use common::{
    AudioCollector, BASIC_ROM_SIZE, CHARACTER_ROM_SIZE, DOS1541_ROM_SIZE, FrameCollector,
    KERNAL_ROM_SIZE, blank_firmware, blank_firmware_with_drive, make_d64, make_tap,
    stub_drive_rom_bytes,
};

#[test]
fn runtime_can_build_from_declared_firmware() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware());
    assert!(runtime.is_ok(), "blank C64 firmware set should construct");
}

#[test]
fn audio_controls_mutate_machine_sid_mixer() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");

    runtime.set_audio_channel_enabled(SidChannel::Voice1, false);
    runtime.set_audio_channel_gain(SidChannel::Voice3, 0.25);

    let controls = runtime.audio_controls();
    assert!(!controls.channel(SidChannel::Voice1).enabled());
    assert_eq!(controls.channel(SidChannel::Voice3).gain(), 0.25);
}

#[test]
fn runtime_can_attach_optional_drive_rom() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
        .expect("blank C64 firmware with optional drive ROM should construct");
    let provider = C64SessionQueryProvider;

    assert!(runtime.drive8().is_some());
    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.attached")
            .expect("drive attachment query should not fail")
            .expect("drive attachment query should resolve")
            .value,
        json!(true)
    );
}

#[test]
fn runtime_run_until_emits_rgba_frame() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = AudioCollector::default();
    let mut trace_sink = NullTraceSink;
    let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame));

    let result = runtime
        .run_until(
            target,
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("blank C64 runtime should run one frame");

    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(result.reached, target);
    assert_eq!(frame_sink.count, 1);
    assert_eq!(frame_sink.last_timestamp, target);
    assert_eq!(frame_sink.last_width, 416);
    assert_eq!(frame_sink.last_height, 312);
    assert_eq!(frame_sink.last_format, Some(PixelFormat::Rgba8888));
    assert_eq!(audio_sink.count, 1);
    assert_eq!(audio_sink.last_timestamp, target);
    assert_eq!(
        audio_sink.last_sample_rate,
        runtime.machine().audio_sample_rate()
    );
    assert_eq!(audio_sink.last_channels, 1);
    assert!(audio_sink.last_samples_len > 0);
}

#[test]
fn runtime_run_until_advances_attached_drive_cycles() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
            .expect("blank C64 firmware with drive should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = AudioCollector::default();
    let mut trace_sink = NullTraceSink;
    let target = MachineTime::new(64);

    runtime
        .run_until(
            target,
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("runtime with an attached drive should run");

    let drive = runtime.drive8().expect("drive should stay attached");
    assert!(drive.cycles() > 0);
    assert!(drive.cpu().regs.pc >= 0xC000);
}

#[test]
fn runtime_rejects_drive_media_without_attached_1541() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let disk = make_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk));

    let err = runtime
        .load_media(&media)
        .expect_err("drive-8 should require an attached 1541 ROM");
    assert!(matches!(
        err,
        MachineError::MissingFirmware { ref id } if id == "commodore-1541-dos-rom"
    ));
}

#[test]
fn runtime_load_media_mounts_d64_into_attached_drive() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware_with_drive())
            .expect("blank C64 firmware with drive should construct a runtime");
    let provider = C64SessionQueryProvider;
    let disk = make_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Disk, &disk));

    runtime
        .load_media(&media)
        .expect("synthetic D64 should mount into the attached 1541");

    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.disk.inserted")
            .expect("disk inserted query should not fail")
            .expect("disk inserted query should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.disk.name")
            .expect("disk name query should not fail")
            .expect("disk name query should resolve")
            .value,
        json!("DEMO DISK")
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.disk.id")
            .expect("disk id query should not fail")
            .expect("disk id query should resolve")
            .value,
        json!("42")
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.disk.write_protected")
            .expect("disk write-protect query should not fail")
            .expect("disk write-protect query should resolve")
            .value,
        json!(true)
    );
    let directory = provider
        .query(&runtime, "c64.drive8.disk.directory")
        .expect("disk directory query should not fail")
        .expect("disk directory query should resolve")
        .value;
    let entries = directory
        .as_array()
        .expect("disk directory should be an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], json!("HELLO"));
    assert_eq!(entries[0]["file_type"], json!("PRG"));
    assert_eq!(entries[0]["blocks"], json!(1));
}

/// NTSC profile selection has its own VIC timing — `to_machine_model`
/// fanned the second `Model` arm into `C64Model::NtscBreadbin`. A blank
/// build should succeed and report NTSC frame dimensions different
/// from the PAL ones.
#[test]
fn runtime_can_build_ntsc_profile() {
    let runtime = C64Runtime::from_firmware(Model::C64NtscBreadbin, &blank_firmware())
        .expect("blank C64 firmware should build NTSC variant");
    let pal = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should build PAL variant");
    assert_ne!(
        runtime.machine().vic().framebuffer_height(),
        pal.machine().vic().framebuffer_height(),
        "NTSC and PAL VIC framebuffers should differ in height"
    );
}

#[test]
fn runtime_rejects_wrong_size_drive_rom() {
    let result = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_ROM_SIZE],
        vec![0; BASIC_ROM_SIZE],
        vec![0; CHARACTER_ROM_SIZE],
        Some(vec![0; DOS1541_ROM_SIZE - 1]),
    );
    let Err(err) = result else {
        panic!("undersized 1541 DOS ROM should be rejected")
    };
    assert!(matches!(
        err,
        MachineError::InvalidFirmware { ref id, .. } if id == "commodore-1541-dos-rom"
    ));
}

#[test]
fn runtime_rejects_wrong_size_kernal_rom() {
    let result = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_ROM_SIZE - 1],
        vec![0; BASIC_ROM_SIZE],
        vec![0; CHARACTER_ROM_SIZE],
        None,
    );
    let Err(err) = result else {
        panic!("undersized KERNAL ROM should be rejected")
    };
    assert!(matches!(
        err,
        MachineError::InvalidFirmware { ref id, .. } if id == "commodore-c64-kernal-rom"
    ));
}

#[test]
fn from_firmware_reports_each_missing_rom() {
    for missing in [
        "commodore-c64-kernal-rom",
        "commodore-c64-basic-rom",
        "commodore-c64-character-rom",
    ] {
        let mut firmware = FirmwareSet::new();
        for id in [
            "commodore-c64-kernal-rom",
            "commodore-c64-basic-rom",
            "commodore-c64-character-rom",
        ] {
            if id == missing {
                continue;
            }
            let bytes: &'static [u8] = match id {
                "commodore-c64-kernal-rom" => {
                    Box::leak(vec![0; KERNAL_ROM_SIZE].into_boxed_slice())
                }
                "commodore-c64-basic-rom" => Box::leak(vec![0; BASIC_ROM_SIZE].into_boxed_slice()),
                "commodore-c64-character-rom" => {
                    Box::leak(vec![0; CHARACTER_ROM_SIZE].into_boxed_slice())
                }
                _ => unreachable!(),
            };
            firmware.push(FirmwareImage::new(id, bytes));
        }

        let result = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware);
        let Err(err) = result else {
            panic!("firmware set without {missing} should be rejected")
        };
        // FirmwareSet validation may report MissingFirmware OR
        // InvalidRequest depending on profile-side checks; both name
        // the missing image.
        let msg = format!("{err:?}");
        assert!(
            msg.contains(missing),
            "error message should name {missing}: {msg}"
        );
    }
}

#[test]
fn machine_core_reset_rebuilds_runtime_state() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    runtime
        .run_until(
            MachineTime::new(64),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("blank runtime should run a few cycles");
    assert_ne!(runtime.time(), MachineTime::default());

    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
    assert_eq!(runtime.machine().phi2_cycles(), 0);
}

#[test]
fn machine_core_capabilities_match_profile() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let caps = runtime.capabilities();
    let profile = runtime_commodore_c64::profile_for(Model::C64PalBreadbin);
    assert_eq!(caps, profile.capabilities);
}

#[test]
fn machine_core_load_media_rejects_unknown_slot() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &[0x00],
    ));

    let err = runtime
        .load_media(&media)
        .expect_err("unknown media slot should be rejected");
    assert!(matches!(
        err,
        MachineError::UnknownMediaSlot { ref slot } if slot == "cartridge-1"
    ));
}

#[test]
fn machine_core_load_media_rejects_wrong_kind_for_tape_slot() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let bytes = make_d64();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Disk, &bytes));

    let err = runtime
        .load_media(&media)
        .expect_err("tape-1 slot should reject Disk kind");
    assert!(matches!(
        err,
        MachineError::UnsupportedMediaKind { kind } if kind == MediaKind::Disk
    ));
}

#[test]
fn machine_core_load_media_rejects_wrong_kind_for_drive_slot() {
    let drive_rom = stub_drive_rom_bytes().to_vec();
    let mut runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_ROM_SIZE],
        vec![0; BASIC_ROM_SIZE],
        vec![0; CHARACTER_ROM_SIZE],
        Some(drive_rom),
    )
    .expect("runtime with drive should construct");
    let bytes = make_tap(&[0x01]);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("drive-8", MediaKind::Tape, &bytes));

    let err = runtime
        .load_media(&media)
        .expect_err("drive-8 slot should reject Tape kind");
    assert!(matches!(
        err,
        MachineError::UnsupportedMediaKind { kind } if kind == MediaKind::Tape
    ));
}

#[test]
fn machine_core_command_rejects_unknown_tape_slot() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let err = runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-2",
            MediaTransportAction::Start,
        )))
        .expect_err("only tape-1 is wired up");
    assert!(matches!(
        err,
        MachineError::UnknownMediaSlot { ref slot } if slot == "tape-2"
    ));
}

/// Trace setters wire diagnostic streams into run_until. The actual
/// emit path requires CPU code mid-loop (live writes to $D020/$D021
/// or a drive ROM PC inside the window), which neither blank ROMs nor
/// the test harness can produce — so we cover the setter lines and
/// confirm enabling trace doesn't break a normal run.
#[derive(Default)]
struct CountingTraceSink {
    events: Vec<String>,
}

impl TraceSink for CountingTraceSink {
    fn push_trace(&mut self, event: TraceEvent<'_>) -> Result<(), MachineError> {
        self.events.push(event.kind.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct DiscardingAudioSink;
impl AudioSink for DiscardingAudioSink {
    fn push_audio(&mut self, _: AudioPacket<'_>) -> Result<(), MachineError> {
        Ok(())
    }
}

#[test]
fn trace_setters_do_not_break_run_until() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    runtime.set_trace_vic_colour_writes(true);
    runtime.set_trace_drive_rom_window(Some((0xC000, 0xC100)));

    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = DiscardingAudioSink;
    let mut trace_sink = CountingTraceSink::default();

    runtime
        .run_until(
            MachineTime::new(64),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("trace-enabled blank runtime should run");

    // Disabling the windows is also part of the API surface.
    runtime.set_trace_vic_colour_writes(false);
    runtime.set_trace_drive_rom_window(None);
}

#[test]
fn run_until_routes_button_input_to_joystick() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let event = InputEvent::Button {
        port: 2,
        name: "fire".into(),
        pressed: true,
    };
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    runtime
        .run_until(
            MachineTime::new(2),
            &mut HostIo {
                input_events: std::slice::from_ref(&event),
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("button event should route through apply_input_event");
}

#[test]
fn file_loader_rejects_unrecognised_extension() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let err = file_loader::load_host_file(&mut runtime, "demo.zip", &[0x50, 0x4B, 0x03, 0x04])
        .expect_err("zip extension is not supported");
    assert!(err.contains("unrecognised file extension"));
    assert!(err.contains("demo.zip"));
}

#[test]
fn run_until_reports_reached_target_with_zero_target() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    let result = runtime
        .run_until(
            MachineTime::default(),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("zero-cycle run should succeed");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(frame_sink.count, 0);
}
