//! NES runtime construction, cartridge load, run-frame, audio mixer,
//! input dispatch, and the optional real-ROM smoke test.

mod common;

use emu198x_shell::{
    ControlCommand, HostIo, InputEvent, MachineCore, MachineError, MachineTime, MediaImage,
    MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, NullAudioSink, NullFrameSink,
    NullTraceSink, ResetKind, StopReason,
};
use runtime_nintendo_nes::{ApuChannel, AudioControls, Model, NesRuntime, profile_for};

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
    let Some(path) = std::env::var_os("EMU198X_NES_SMB_ROM").map(std::path::PathBuf::from) else {
        eprintln!("SKIPPING: set EMU198X_NES_SMB_ROM to a Super Mario Bros. iNES path");
        return;
    };
    if !path.is_file() {
        eprintln!("SKIPPING: SMB ROM not found at {}", path.display());
        return;
    }

    let rom = std::fs::read(&path).expect("reference ROM should read");
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

/// Blank runtime accessors should report "no machine yet" instead of
/// blowing up. `audio_controls` is the only optional getter; the
/// setters are no-ops on a blank runtime.
#[test]
fn blank_runtime_has_no_machine_or_audio_controls() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    assert!(runtime.machine().is_none());
    assert!(runtime.machine_mut().is_none());
    assert!(runtime.audio_controls().is_none());
    // Setters should silently no-op without a cartridge attached.
    runtime.set_audio_channel_enabled(ApuChannel::Pulse1, false);
    runtime.set_audio_channel_gain(ApuChannel::Dmc, 0.5);
    runtime.set_audio_controls(AudioControls::default());
    assert!(runtime.audio_controls().is_none());
}

/// `set_audio_controls` replaces all controls when a cartridge is
/// attached. The other audio setters already have a coverage test;
/// this one drives the bulk-replace path.
#[test]
fn set_audio_controls_applies_when_cartridge_loaded() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut controls = AudioControls::default();
    controls.set_channel_enabled(ApuChannel::Triangle, false);
    controls.set_channel_gain(ApuChannel::Noise, 0.5);
    runtime.set_audio_controls(controls);

    let after = runtime.audio_controls().expect("loaded cartridge");
    assert!(!after.channel(ApuChannel::Triangle).enabled());
    assert_eq!(after.channel(ApuChannel::Noise).gain(), 0.5);
}

/// `run_until` short-circuits when no cartridge is loaded; the
/// runtime reports `WaitingForInput` without advancing time.
#[test]
fn run_until_returns_waiting_for_input_when_blank() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
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
        .expect("blank runtime should not error");
    assert_eq!(result.stop_reason, StopReason::WaitingForInput);
    assert_eq!(runtime.time(), MachineTime::default());
}

/// Loading a non-iNES blob through the cartridge slot rebrands the
/// parser error as `MachineError::InvalidMedia` with the slot name.
#[test]
fn load_media_rejects_invalid_ines_bytes() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new(
        "cartridge-1",
        MediaKind::Cartridge,
        &[0x00, 0x01, 0x02],
    ));
    let err = runtime
        .load_media(&media)
        .expect_err("garbage bytes should not parse as iNES");
    assert!(matches!(
        err,
        MachineError::InvalidMedia { ref slot, .. } if slot == "cartridge-1"
    ));
    // The runtime stays blank after a failed load.
    assert!(runtime.machine().is_none());
}

#[test]
fn load_media_rejects_unknown_slot() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-2", MediaKind::Cartridge, &rom));
    let err = runtime
        .load_media(&media)
        .expect_err("only cartridge-1 is wired up");
    assert!(matches!(
        err,
        MachineError::UnknownMediaSlot { ref slot } if slot == "cartridge-2"
    ));
}

#[test]
fn load_media_rejects_wrong_kind_for_cartridge_slot() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Disk, &[0x00]));
    let err = runtime
        .load_media(&media)
        .expect_err("cartridge slot should reject Disk kind");
    assert!(matches!(
        err,
        MachineError::UnsupportedMediaKind { kind } if kind == MediaKind::Disk
    ));
}

/// `MachineCore::reset` runs the rebuild path: when a cartridge is
/// loaded the machine is rebuilt from the cached iNES bytes and the
/// time stamp resets to zero.
#[test]
fn reset_rebuilds_machine_from_cached_cartridge_bytes() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("one frame should run");
    assert_ne!(runtime.time(), MachineTime::default());

    runtime.reset(ResetKind::Hard);
    assert_eq!(runtime.time(), MachineTime::default());
    assert!(runtime.machine().is_some(), "cartridge should reload");
    assert_eq!(
        runtime.machine().expect("reloaded").frame_count(),
        0,
        "machine should be re-newed from iNES bytes",
    );

    // Soft reset hits the same rebuild path (kind is ignored).
    runtime.reset(ResetKind::Soft);
    assert_eq!(runtime.time(), MachineTime::default());
}

/// Reset against a blank runtime (no cached cartridge bytes) should
/// drive the early-return arm in `rebuild_loaded_machine`.
#[test]
fn reset_on_blank_runtime_stays_blank() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    runtime.reset(ResetKind::Hard);
    assert!(runtime.machine().is_none());
    assert_eq!(runtime.time(), MachineTime::default());
}

/// A snapshot whose embedded `cartridge_bytes` no longer parse as
/// iNES drives the `parse_ines` `Err` arm in `rebuild_loaded_machine`.
/// We can't legitimately reach that state through `load_media` (it
/// validates first), but a hand-crafted snapshot can — postcard
/// decode doesn't reparse the cartridge bytes. After restore, a
/// reset triggers the rebuild and lands on the error arm, which
/// nulls out the machine instead of panicking.
#[test]
fn reset_with_corrupt_cached_cartridge_bytes_blanks_machine() {
    use postcard::to_allocvec;
    use serde::Serialize;

    #[derive(Serialize)]
    struct ManualEnvelope<'a> {
        version: u16,
        time: u64,
        cartridge_mapper: Option<u16>,
        cartridge_bytes: Option<&'a [u8]>,
        machine: Option<()>,
    }

    let bytes = to_allocvec(&ManualEnvelope {
        version: 1,
        time: 0,
        cartridge_mapper: Some(0),
        // Not iNES — parse_ines will reject during reset's rebuild.
        cartridge_bytes: Some(&[0x00, 0x01, 0x02, 0x03]),
        machine: None,
    })
    .expect("postcard should encode our manual envelope");

    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    runtime
        .restore(&bytes)
        .expect("manual envelope should decode (parse_ines is not called yet)");

    runtime.reset(ResetKind::Hard);
    assert!(
        runtime.machine().is_none(),
        "reset should null out the machine when cached bytes no longer parse",
    );
}

/// `MachineCore::profile` and `capabilities` reflect the constructor
/// model. `command` always rejects with `UnsupportedOperation` because
/// the NES runtime does not wire up tape / disk transport controls.
#[test]
fn machine_core_profile_and_capabilities_match_profile_for() {
    let runtime = NesRuntime::blank(Model::NesNtsc);
    let expected = profile_for(Model::NesNtsc);
    assert_eq!(runtime.profile().profile_id, expected.profile_id);
    assert_eq!(runtime.capabilities(), expected.capabilities);
}

#[test]
fn command_reports_unsupported_operation() {
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let err = runtime
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            "cartridge-1",
            MediaTransportAction::Start,
        )))
        .expect_err("NES runtime does not support transport commands");
    assert!(matches!(
        err,
        MachineError::UnsupportedOperation { operation } if operation == "media-transport"
    ));
}

/// Key events route through `apply_input_event` the same way port-1
/// button events do. Driving a key press confirms the `InputEvent::Key`
/// arm and the press/release branches both update controller 1.
#[test]
fn key_events_route_to_controller1_press_and_release() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let press = [InputEvent::Key {
        name: "A".into(),
        pressed: true,
    }];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &press,
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("press should run");
    assert_eq!(
        runtime.machine().expect("loaded").controller1_state & 0x01,
        0x01,
        "uppercase 'A' should still set bit 0 (case-insensitive lookup)",
    );

    let release = [InputEvent::Key {
        name: "a".into(),
        pressed: false,
    }];
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS * 2),
            &mut HostIo {
                input_events: &release,
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("release should run");
    assert_eq!(
        runtime.machine().expect("loaded").controller1_state & 0x01,
        0,
        "release should clear bit 0",
    );
}

/// Every NES button name maps to a distinct controller-1 bit. Driving
/// each press in turn covers each arm of `button_bit`.
#[test]
fn every_button_name_maps_to_distinct_bit() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;

    let mut tick = 0u64;
    for (name, bit) in [
        ("a", 0u8),
        ("b", 1),
        ("select", 2),
        ("start", 3),
        ("up", 4),
        ("down", 5),
        ("left", 6),
        ("right", 7),
    ] {
        let events = [InputEvent::Button {
            port: 1,
            name: name.into(),
            pressed: true,
        }];
        tick += NTSC_FRAME_TICKS;
        runtime
            .run_until(
                MachineTime::new(tick),
                &mut HostIo {
                    input_events: &events,
                    frame_sink: &mut frame_sink,
                    audio_sink: &mut audio_sink,
                    trace_sink: &mut trace_sink,
                },
            )
            .expect("press frame should run");
        let mask = 1u8 << bit;
        assert_eq!(
            runtime.machine().expect("loaded").controller1_state & mask,
            mask,
            "button {name} should set bit {bit}",
        );
    }
}

/// Unknown button names are silently dropped (`button_bit` returns
/// `None`); controller state stays unchanged.
#[test]
fn unknown_button_name_does_not_alter_controller_state() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let events = [InputEvent::Button {
        port: 1,
        name: "turbo".into(),
        pressed: true,
    }];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &events,
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("frame should run");
    assert_eq!(
        runtime.machine().expect("loaded").controller1_state,
        0,
        "unknown button should not alter controller state",
    );
}

/// Port-2 button events and unrelated event kinds are ignored. The
/// catch-all `_ => {}` arm in `apply_input_event` swallows them.
#[test]
fn non_port1_button_and_other_events_are_ignored() {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media).expect("valid iNES should load");

    let events = [
        InputEvent::Button {
            port: 2,
            name: "a".into(),
            pressed: true,
        },
        InputEvent::PointerMotion {
            device: "mouse-1".into(),
            dx: 1,
            dy: -1,
        },
    ];
    let mut frame_sink = NullFrameSink;
    let mut audio_sink = NullAudioSink;
    let mut trace_sink = NullTraceSink;
    runtime
        .run_until(
            MachineTime::new(NTSC_FRAME_TICKS),
            &mut HostIo {
                input_events: &events,
                frame_sink: &mut frame_sink,
                audio_sink: &mut audio_sink,
                trace_sink: &mut trace_sink,
            },
        )
        .expect("frame should run");
    assert_eq!(
        runtime.machine().expect("loaded").controller1_state,
        0,
        "port-2 and pointer events should not alter controller 1",
    );
}
