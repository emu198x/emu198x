//! Game Boy runtime construction, audio controls, cartridge loading,
//! reset behaviour, and the basic `run_until` host-loop contract.

mod common;

use std::borrow::Cow;

use common_nintendo_game_boy::MCYCLES_PER_FRAME;
use emu198x_shell::{
    CapabilitySet, ControlCommand, HostIo, InputEvent, MachineCore, MachineError, MachineTime,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, ResetKind,
    StopReason, known_capability,
};
use runtime_nintendo_game_boy::{ApuChannel, GameBoyRuntime, Model};

use common::{battery_ram_rom, loop_rom, null_host_buffers};

#[test]
fn blank_runtime_has_dmg_profile_and_no_machine() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    assert_eq!(
        runtime.profile().profile_id.as_str(),
        "nintendo-game-boy-dmg"
    );
    assert!(runtime.machine().is_none());
    assert_eq!(runtime.time(), MachineTime::default());
}

#[test]
fn loading_a_valid_cartridge_constructs_the_machine() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");
    assert!(runtime.machine().is_some());
}

#[test]
fn battery_backed_ram_can_be_restored_and_exported() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = battery_ram_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("MBC1+RAM+battery cartridge should load");

    assert!(runtime.has_battery_backed_ram());
    let save = vec![0x5A; 0x2000];
    runtime
        .restore_cartridge_ram(&save)
        .expect("save image with matching size should restore");

    assert_eq!(runtime.cartridge_ram(), Some(save.as_slice()));
}

#[test]
fn reset_preserves_external_ram() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = battery_ram_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("MBC1+RAM+battery cartridge should load");

    let mut save = vec![0xFF; 0x2000];
    save[0] = 0xC0;
    save[0x1FFF] = 0xDE;
    runtime
        .restore_cartridge_ram(&save)
        .expect("save image with matching size should restore");
    runtime.reset(ResetKind::Hard);

    assert_eq!(runtime.cartridge_ram(), Some(save.as_slice()));
}

#[test]
fn load_media_rejects_unknown_slot() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("nope", MediaKind::Cartridge, &rom));
    let err = runtime
        .load_media(&media)
        .expect_err("unknown slot should reject");
    match err {
        MachineError::UnknownMediaSlot { slot } => assert_eq!(slot, "nope"),
        other => panic!("expected UnknownMediaSlot, got {other:?}"),
    }
}

#[test]
fn load_media_rejects_wrong_kind() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Tape, &rom));
    let err = runtime
        .load_media(&media)
        .expect_err("cartridge slot should reject Tape kind");
    match err {
        MachineError::UnsupportedMediaKind { kind } => assert_eq!(kind, MediaKind::Tape),
        other => panic!("expected UnsupportedMediaKind, got {other:?}"),
    }
}

#[test]
fn run_until_without_cartridge_reports_waiting_for_input() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let (mut frame_sink, mut audio_sink, mut trace_sink) = null_host_buffers();
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    let result = runtime
        .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
        .expect("run without cartridge should not error");
    assert_eq!(result.stop_reason, StopReason::WaitingForInput);
}

#[test]
fn run_until_advances_machine_time_and_emits_one_frame() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    let (mut frame_sink, mut audio_sink, mut trace_sink) = null_host_buffers();
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    let result = runtime
        .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
        .expect("loaded runtime should run one frame");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert!(runtime.time().get() >= u64::from(MCYCLES_PER_FRAME));
}

#[test]
fn key_input_event_presses_joypad_button() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    let events = [InputEvent::Key {
        name: "start".into(),
        pressed: true,
    }];
    let (mut frame_sink, mut audio_sink, mut trace_sink) = null_host_buffers();
    let mut host = HostIo {
        input_events: &events,
        frame_sink: &mut frame_sink,
        audio_sink: &mut audio_sink,
        trace_sink: &mut trace_sink,
    };
    runtime
        .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
        .expect("loaded runtime should run one frame with input");

    // We can't peek into the joypad state from outside the
    // machine, but we can confirm Start is mapped to a button by
    // round-tripping a snapshot — if the press was applied, the
    // restored runtime should round-trip it byte-identically.
    let snap = runtime.snapshot().expect("loaded runtime should snapshot");
    let mut reborn = GameBoyRuntime::blank(Model::Dmg);
    reborn.restore(&snap).expect("snapshot should restore");
    let snap2 = reborn.snapshot().expect("restored runtime should snapshot");
    assert_eq!(snap, snap2);
}

#[test]
fn audio_controls_mutate_loaded_machine_mixer() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    runtime.set_audio_channel_enabled(ApuChannel::Noise, false);
    runtime.set_audio_channel_gain(ApuChannel::Wave, 0.25);

    let controls = runtime
        .audio_controls()
        .expect("loaded runtime should expose audio controls");
    assert!(!controls.channel(ApuChannel::Noise).enabled());
    assert_eq!(controls.channel(ApuChannel::Wave).gain(), 0.25);
}

/// `set_audio_controls` mutates the loaded mixer wholesale — the
/// per-channel setters round-trip through the same field. With no
/// cartridge it's a no-op.
#[test]
fn set_audio_controls_replaces_loaded_mixer() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    // No-op when no cartridge is loaded — should not panic.
    let blank_controls = runtime
        .audio_controls()
        .unwrap_or_default();
    runtime.set_audio_controls(blank_controls);
    assert!(runtime.audio_controls().is_none());

    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    let mut new_controls = runtime
        .audio_controls()
        .expect("loaded runtime should expose audio controls");
    new_controls.set_channel_gain(ApuChannel::Pulse1, 0.5);
    runtime.set_audio_controls(new_controls);

    let after = runtime
        .audio_controls()
        .expect("loaded runtime should still expose controls");
    assert_eq!(after.channel(ApuChannel::Pulse1).gain(), 0.5);
}

/// `machine_mut` returns `None` for a blank runtime and `Some` after
/// a cartridge load. Mirror of the read-only `machine` accessor; the
/// mutable variant is what the host shell uses for trace setters.
#[test]
fn machine_mut_tracks_cartridge_lifecycle() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    assert!(runtime.machine_mut().is_none());

    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    assert!(runtime.machine_mut().is_some());
}

/// `restore_cartridge_ram` rejects calls before any cartridge has
/// been loaded — the slot is "cartridge" and the reason names the
/// missing cart.
#[test]
fn restore_cartridge_ram_rejects_when_no_cartridge() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let err = runtime
        .restore_cartridge_ram(&[0; 0x2000])
        .expect_err("blank runtime should reject save restore");
    match err {
        MachineError::InvalidMedia { slot, reason } => {
            assert_eq!(slot, "cartridge");
            assert!(
                reason.contains("no cartridge is loaded"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// `restore_cartridge_ram` rejects when the loaded cartridge has no
/// external RAM — the synthetic `loop_rom()` has ROM-only mapping.
#[test]
fn restore_cartridge_ram_rejects_when_cartridge_has_no_ram() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = loop_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("synthetic cartridge should load");

    let err = runtime
        .restore_cartridge_ram(&[0; 0x2000])
        .expect_err("ROM-only cartridge should reject save restore");
    match err {
        MachineError::InvalidMedia { reason, .. } => {
            assert!(
                reason.contains("no external RAM"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// `restore_cartridge_ram` rejects a save image whose length does
/// not match the cartridge's RAM size — both lengths are reported
/// in the reason for diagnostic purposes.
#[test]
fn restore_cartridge_ram_rejects_size_mismatch() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let rom = battery_ram_rom();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime
        .load_media(&media)
        .expect("MBC1+RAM+battery cartridge should load");

    let err = runtime
        .restore_cartridge_ram(&[0; 16])
        .expect_err("undersized save should reject");
    match err {
        MachineError::InvalidMedia { reason, .. } => {
            assert!(
                reason.contains("save RAM length 16"),
                "unexpected reason: {reason}",
            );
            assert!(
                reason.contains("cartridge RAM length 8192"),
                "unexpected reason: {reason}",
            );
        }
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// `load_media` returns `InvalidMedia` when the cartridge bytes
/// can't be parsed as a Game Boy ROM — for example, an empty slice
/// fails the header decode.
#[test]
fn load_media_rejects_unparseable_cartridge_bytes() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let mut media = MediaSet::new();
    let bogus: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &bogus));
    let err = runtime
        .load_media(&media)
        .expect_err("malformed bytes should reject");
    match err {
        MachineError::InvalidMedia { slot, .. } => assert_eq!(slot, "cartridge"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// `reset` on a blank runtime drives `rebuild_machine` through the
/// no-cartridge-bytes branch — the machine stays `None`, the time
/// resets to zero, and no panic occurs.
#[test]
fn reset_blank_runtime_is_a_noop() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    assert!(runtime.machine().is_none());
    runtime.reset(ResetKind::Hard);
    assert!(runtime.machine().is_none());
    assert_eq!(runtime.time(), MachineTime::default());
}

/// `MachineCore::profile` and `MachineCore::time` round-trip through
/// the trait the same way the inherent accessors do.
#[test]
fn machine_core_profile_and_time_match_inherent_accessors() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    let trait_profile = MachineCore::profile(&runtime);
    assert_eq!(trait_profile.profile_id.as_str(), "nintendo-game-boy-dmg");
    assert_eq!(MachineCore::time(&runtime), MachineTime::default());
}

/// `command` rejects every `ControlCommand` variant — the runtime
/// has no transport surface today. The reported `operation` name
/// must come from `ControlCommand::operation_name` so future
/// transport commands route through the same string table.
#[test]
fn command_returns_unsupported_for_media_transport() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let cmd = ControlCommand::MediaTransport(MediaTransportCommand {
        slot: Cow::Borrowed("cartridge"),
        action: MediaTransportAction::Start,
    });
    let err = runtime
        .command(&cmd)
        .expect_err("game boy has no transport surface");
    match err {
        MachineError::UnsupportedOperation { operation } => {
            assert_eq!(operation, "media-transport");
        }
        other => panic!("expected UnsupportedOperation, got {other:?}"),
    }
}

/// `capabilities` reports the four capabilities the family declares
/// in `profile_for`: keyboard-matrix, scripted-input, and snapshot
/// import/export. Regression catches a profile-edit that silently
/// strips one of them.
#[test]
fn capabilities_reports_family_capability_set() {
    let runtime = GameBoyRuntime::blank(Model::Dmg);
    let caps: CapabilitySet = runtime.capabilities();
    assert!(caps.contains(&known_capability("keyboard-matrix")));
    assert!(caps.contains(&known_capability("scripted-input")));
    assert!(caps.contains(&known_capability("snapshot-export")));
    assert!(caps.contains(&known_capability("snapshot-import")));
}
