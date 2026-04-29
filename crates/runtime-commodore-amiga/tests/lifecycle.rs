//! Amiga runtime construction, audio controls, ADF media loading,
//! reset behaviour, and the basic `run_until` host-loop contract.

mod common;

use emu198x_shell::{
    HostIo, InputEvent, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet,
    NullFrameSink, NullTraceSink,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{A500_PAL_FRAME_TICKS, AmigaRuntime, Model, PaulaChannel};

use common::{
    A1000_BOOTSTRAP_ROM_ID, AUDIO_CHANNELS, AUDIO_SAMPLE_RATE_HZ, AudioCollector,
    KICKSTART_ROM_ID, audio_sample_frames_for_ticks, dummy_a1000_bootstrap_rom, dummy_a1000_firmware,
    dummy_firmware, dummy_kickstart,
};

#[test]
fn from_firmware_accepts_supported_kickstart_size() {
    let runtime = AmigaRuntime::from_firmware(Model::A500OcsPal, &dummy_firmware());
    assert!(runtime.is_ok());
}

#[test]
fn audio_controls_mutate_machine_paula_mixer() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");

    runtime.set_audio_channel_enabled(PaulaChannel::Channel1, false);
    runtime.set_audio_channel_gain(PaulaChannel::Channel3, 0.25);

    let controls = runtime.audio_controls();
    assert!(!controls.channel(PaulaChannel::Channel1).enabled());
    assert_eq!(controls.channel(PaulaChannel::Channel3).gain(), 0.25);
}

#[test]
fn from_firmware_accepts_supported_a1000_bootstrap_size() {
    let runtime = AmigaRuntime::from_firmware(Model::A1000OcsPal, &dummy_a1000_firmware());
    assert!(runtime.is_ok());
}

#[test]
fn new_rejects_undersized_rom() {
    match AmigaRuntime::new(Model::A500OcsPal, vec![0; 128 * 1024]) {
        Err(MachineError::InvalidFirmware { id, .. }) => assert_eq!(id, KICKSTART_ROM_ID),
        Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
        Ok(_) => panic!("expected InvalidFirmware, got Ok"),
    }
}

#[test]
fn a1000_new_rejects_non_bootstrap_rom_size() {
    match AmigaRuntime::new(Model::A1000OcsPal, vec![0; 256 * 1024]) {
        Err(MachineError::InvalidFirmware { id, .. }) => {
            assert_eq!(id, A1000_BOOTSTRAP_ROM_ID)
        }
        Err(other) => panic!("expected InvalidFirmware, got {other:?}"),
        Ok(_) => panic!("expected InvalidFirmware, got Ok"),
    }
}

#[test]
fn load_media_accepts_dd_adf() {
    let mut runtime = AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart())
        .expect("dummy Kickstart should construct");
    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime
        .load_media(&media)
        .expect("ADF bytes should insert into DF0");
    assert!(runtime.machine().drive().has_disk());
}

#[test]
fn load_media_keeps_a1000_disk_change_pending() {
    let mut runtime = AmigaRuntime::new(Model::A1000OcsPal, dummy_a1000_bootstrap_rom())
        .expect("dummy bootstrap ROM should construct");
    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime
        .load_media(&media)
        .expect("ADF bytes should insert into DF0");

    assert!(runtime.machine().drive().has_disk());
    assert!(
        runtime.machine().drive().status().disk_change,
        "A1000 bootstrap expects a fresh /DSKCHANGE event when Kickstart media is loaded"
    );
}

#[test]
fn load_media_rejects_unknown_slot() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-1", MediaKind::Disk, &disk));
    let err = runtime.load_media(&media).expect_err("unknown slot");
    matches!(err, MachineError::UnknownMediaSlot { .. });
}

#[test]
fn run_until_advances_time_and_emits_frame() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let target = MachineTime::new(A500_PAL_FRAME_TICKS);
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = AudioCollector::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(target, &mut host)
        .expect("one frame should run");
    assert_eq!(runtime.time(), target);
    assert_eq!(audio_sink.packets, 1);
    assert_eq!(audio_sink.last_timestamp, target);
    assert_eq!(audio_sink.last_sample_rate, AUDIO_SAMPLE_RATE_HZ);
    assert_eq!(audio_sink.last_channels, AUDIO_CHANNELS);
    assert_eq!(
        audio_sink.last_samples.len(),
        audio_sample_frames_for_ticks(A500_PAL_FRAME_TICKS) * usize::from(AUDIO_CHANNELS)
    );
    assert!(
        audio_sink
            .last_samples
            .iter()
            .all(|sample| sample.is_finite())
    );
}

#[test]
fn run_until_applies_mouse_input_to_controller_port_zero() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let input_events = [
        InputEvent::PointerMotion {
            device: "mouse-1".into(),
            dx: 3,
            dy: 4,
        },
        InputEvent::PointerButton {
            device: "mouse-1".into(),
            button: "left".into(),
            pressed: true,
        },
    ];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = AudioCollector::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &input_events,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    runtime
        .run_until(MachineTime::new(A500_PAL_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    assert_eq!(runtime.machine().read_word(0x00DF_F00A), 0x0403);
    assert_eq!(runtime.machine().read_word(0x00BF_E001) & 0x80, 0);
}

#[test]
fn run_until_applies_joystick_input_to_controller_port_one() {
    let mut runtime =
        AmigaRuntime::new(Model::A500OcsPal, dummy_kickstart()).expect("runtime init");
    let input_events = [
        InputEvent::Button {
            port: 1,
            name: "right".into(),
            pressed: true,
        },
        InputEvent::Button {
            port: 1,
            name: "fire".into(),
            pressed: true,
        },
    ];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = AudioCollector::default();
    let mut trace_sink = NullTraceSink;
    let mut host = HostIo {
        input_events: &input_events,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };

    runtime
        .run_until(MachineTime::new(A500_PAL_FRAME_TICKS), &mut host)
        .expect("one frame should run");

    assert_eq!(runtime.machine().read_word(0x00DF_F00C) & 0x0003, 0x0003);
    assert_eq!(runtime.machine().read_word(0x00BF_E001) & 0x40, 0);
}
