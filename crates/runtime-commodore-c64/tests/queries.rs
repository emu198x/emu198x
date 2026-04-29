//! Query-provider coverage for the C64 runtime.
//!
//! Hermetic tests run on every workspace test invocation. The
//! `#[ignore]`'d real-ROM tests resolve assets from
//! `~/.emu198x/roms/commodore-c64/` and the local Reference library.

mod common;

use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::{
    ControlCommand, HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet,
    MediaTransportAction, MediaTransportCommand, NullAudioSink, NullTraceSink,
    SessionQueryProvider,
};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider, Model};
use serde_json::json;

use common::{
    FrameCollector, SCREEN_TEXT_HEIGHT, blank_firmware, local_rom_firmware,
    local_rom_firmware_with_drive, make_tap,
};

#[test]
fn query_provider_reports_blank_runtime_as_not_booted() {
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let provider = C64SessionQueryProvider;

    let paths = provider.query_paths(&runtime, Some("boot."));
    assert_eq!(
        paths,
        vec![
            "boot.detected".to_owned(),
            "boot.offset".to_owned(),
            "boot.reason".to_owned(),
            "boot.row".to_owned(),
        ]
    );

    let detected = provider
        .query(&runtime, "boot.detected")
        .expect("boot.detected query should not fail")
        .expect("boot.detected should resolve");
    assert_eq!(detected.value, json!(false));

    let reason = provider
        .query(&runtime, "boot.reason")
        .expect("boot.reason query should not fail")
        .expect("boot.reason should resolve");
    assert_eq!(reason.value, json!("READY. screen codes not visible"));

    let row = provider
        .query(&runtime, "boot.row")
        .expect("boot.row query should not fail")
        .expect("boot.row should resolve");
    assert_eq!(row.value, json!(null));

    let tape_loaded = provider
        .query(&runtime, "c64.tape.loaded")
        .expect("c64.tape.loaded query should not fail")
        .expect("c64.tape.loaded should resolve");
    assert_eq!(tape_loaded.value, json!(false));

    let text_lines = provider
        .query(&runtime, "screen.text.lines")
        .expect("screen.text.lines query should not fail")
        .expect("screen.text.lines should resolve");
    let lines = text_lines
        .value
        .as_array()
        .expect("screen.text.lines should be an array");
    assert_eq!(lines.len(), SCREEN_TEXT_HEIGHT);
    assert!(
        lines
            .iter()
            .all(|line| line.as_str().is_some_and(|line| line.len() == 40))
    );

    assert!(matches!(provider.query(&runtime, "not-a-path"), Ok(None)));
}

#[test]
fn runtime_load_media_and_transport_update_tape_queries() {
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &blank_firmware())
        .expect("blank C64 firmware should construct a runtime");
    let tape = make_tap(&[0x01, 0x01]);
    let provider = C64SessionQueryProvider;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &tape));

    runtime
        .load_media(&media)
        .expect("synthetic TAP should load through runtime");
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.loaded")
            .expect("c64.tape.loaded query should not fail")
            .expect("c64.tape.loaded should resolve")
            .value,
        json!(true)
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.playing")
            .expect("c64.tape.playing query should not fail")
            .expect("c64.tape.playing should resolve")
            .value,
        json!(false)
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.sense")
            .expect("c64.tape.sense query should not fail")
            .expect("c64.tape.sense should resolve")
            .value,
        json!(false)
    );

    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Start,
        )))
        .expect("tape transport should start");
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.playing")
            .expect("c64.tape.playing query should not fail")
            .expect("c64.tape.playing should resolve")
            .value,
        json!(false)
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.sense")
            .expect("c64.tape.sense query should not fail")
            .expect("c64.tape.sense should resolve")
            .value,
        json!(true)
    );

    runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "tape-1",
            MediaTransportAction::Stop,
        )))
        .expect("tape transport should stop");
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.playing")
            .expect("c64.tape.playing query should not fail")
            .expect("c64.tape.playing should resolve")
            .value,
        json!(false)
    );
    assert_eq!(
        provider
            .query(&runtime, "c64.tape.sense")
            .expect("c64.tape.sense query should not fail")
            .expect("c64.tape.sense should resolve")
            .value,
        json!(false)
    );
}

#[test]
#[ignore = "requires local C64 and 1541 ROMs at ~/.emu198x/roms/commodore-c64"]
fn query_provider_reports_real_attached_drive_progress() {
    let mut runtime =
        C64Runtime::from_firmware(Model::C64PalBreadbin, &local_rom_firmware_with_drive())
            .expect("local ROMs should construct a C64 runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let provider = C64SessionQueryProvider;

    runtime
        .run_until(
            MachineTime::new(512),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("real ROM-backed runtime should run");

    assert_eq!(
        provider
            .query(&runtime, "c64.drive8.attached")
            .expect("drive attachment query should not fail")
            .expect("drive attachment query should resolve")
            .value,
        json!(true)
    );
    let drive_cycles = provider
        .query(&runtime, "c64.drive8.cpu.cycles")
        .expect("drive cycle query should not fail")
        .expect("drive cycle query should resolve")
        .value
        .as_u64()
        .expect("drive cycles should be a u64");
    assert!(drive_cycles > 0);
}

#[test]
#[ignore = "requires local C64 ROMs at ~/.emu198x/roms/commodore-c64"]
fn query_provider_detects_ready_on_real_pal_boot() {
    let firmware = local_rom_firmware();
    let mut runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut frame_sink = FrameCollector::default();
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let target = MachineTime::new(u64::from(TIMING_PAL_BREADBIN.cycles_per_frame) * 200);

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
        .expect("real PAL C64 ROMs should run to boot window");

    let provider = C64SessionQueryProvider;
    let detected = provider
        .query(&runtime, "boot.detected")
        .expect("boot.detected query should not fail")
        .expect("boot.detected should resolve");
    assert_eq!(detected.value, json!(true));

    let offset = provider
        .query(&runtime, "boot.offset")
        .expect("boot.offset query should not fail")
        .expect("boot.offset should resolve");
    assert_ne!(offset.value, json!(null));

    let row = provider
        .query(&runtime, "boot.row")
        .expect("boot.row query should not fail")
        .expect("boot.row should resolve");
    assert_ne!(row.value, json!(null));
}
