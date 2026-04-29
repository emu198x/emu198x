//! Snapshot/restore round-trip coverage for the Game Boy runtime.

mod common;

use common_nintendo_game_boy::MCYCLES_PER_FRAME;
use emu198x_shell::{HostIo, MachineCore, MachineError, MachineTime};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};

use common::{loop_rom, null_host_buffers};
use emu198x_shell::{MediaImage, MediaKind, MediaSet};

#[test]
fn snapshot_round_trip_preserves_state() {
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
    runtime
        .run_until(MachineTime::new(u64::from(MCYCLES_PER_FRAME)), &mut host)
        .expect("loaded runtime should run one frame");

    let snap = runtime.snapshot().expect("loaded runtime should snapshot");
    let mut reborn = GameBoyRuntime::blank(Model::Dmg);
    reborn.restore(&snap).expect("snapshot should restore");
    assert_eq!(reborn.time(), runtime.time());
    assert!(reborn.machine().is_some());
}

/// Profile mismatch is the safety belt that prevents a DMG snapshot
/// from being restored into a DMG0 runtime (or any other model in
/// the family). Build a runtime in one model, snapshot it, then try
/// to restore into the other model.
#[test]
fn restore_rejects_snapshot_from_different_profile() {
    let dmg = GameBoyRuntime::blank(Model::Dmg);
    let snapshot = dmg.snapshot().expect("blank DMG runtime should snapshot");

    let mut dmg0 = GameBoyRuntime::blank(Model::Dmg0);
    let err = dmg0
        .restore(&snapshot)
        .expect_err("DMG0 runtime should refuse a DMG-profile snapshot");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("profile")),
        "unexpected error variant: {err:?}",
    );
}

#[test]
fn restore_rejects_corrupt_postcard_bytes() {
    let mut runtime = GameBoyRuntime::blank(Model::Dmg);
    let err = runtime
        .restore(&[0xFF, 0xFF, 0xFF, 0xFF])
        .expect_err("garbage bytes should not deserialise");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("decode failed")),
        "unexpected error variant: {err:?}",
    );
}
