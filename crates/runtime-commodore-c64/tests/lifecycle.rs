//! C64 runtime construction, audio controls, drive attachment, and
//! the basic synthetic-media load path.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullTraceSink,
    PixelFormat, SessionQueryProvider, StopReason,
};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model, SidChannel};
use serde_json::json;

use common::{
    AudioCollector, FrameCollector, blank_firmware, blank_firmware_with_drive, make_d64,
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
