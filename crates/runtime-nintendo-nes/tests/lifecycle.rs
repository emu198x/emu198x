//! NES runtime construction, cartridge load, run-frame, audio mixer,
//! input dispatch, and the optional real-ROM smoke test.

mod common;

use std::path::Path;

use emu198x_shell::{
    HostIo, InputEvent, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink, StopReason,
};
use runtime_nintendo_nes::{ApuChannel, Model, NesRuntime};

use common::{NTSC_FRAME_TICKS, minimal_ines};

#[test]
fn runtime_loads_cartridge_and_runs_one_frame() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    let result = runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert!(runtime.time() >= MachineTime::new(NTSC_FRAME_TICKS));
    assert_eq!(
        runtime.machine().expect("cartridge loaded").frame_count(),
        1
    );
}

#[test]
fn button_input_updates_controller_state() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let events = [InputEvent::Button {
        port: 1,
        name: "start".into(),
        pressed: true,
    }];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &events,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    assert_eq!(
        runtime
            .machine()
            .expect("cartridge loaded")
            .controller1_state
            & (1 << 3),
        1 << 3
    );
}

#[test]
fn audio_controls_mutate_loaded_machine_mixer() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    runtime.set_audio_channel_enabled(ApuChannel::Pulse1, false);
    runtime.set_audio_channel_gain(ApuChannel::Dmc, 0.25);

    let controls = runtime
        .audio_controls()
        .expect("audio controls should exist for loaded cartridge");
    assert!(!controls.channel(ApuChannel::Pulse1).enabled());
    assert_eq!(controls.channel(ApuChannel::Dmc).gain(), 0.25);
}

#[test]
#[ignore = "uses local NES reference ROM"]
fn real_ines_super_mario_bros_runs_and_draws() {
    let path = Path::new(
        "/Users/stevehill/Projects/Emu198x-Unclean/Reference/nintendo/nes/Super Mario Bros. (1985-09-13)(Nintendo)(JP-US).nes",
    );
    if !path.is_file() {
        eprintln!("SKIPPING: local Super Mario Bros. ROM not found");
        return;
    }

    let rom = std::fs::read(path).expect("reference ROM should read");
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("reference iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    runtime
        .run_until(MachineTime::new(NTSC_FRAME_TICKS * 240), &mut host)
        .expect("reference ROM should run");

    let machine = runtime.machine().expect("cartridge should remain loaded");
    assert!(machine.frame_count() > 0);
    assert!(machine.framebuffer().iter().any(|&pixel| pixel != 0));
}
