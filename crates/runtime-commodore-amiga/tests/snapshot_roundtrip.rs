//! Round-trip determinism tests for the Amiga snapshot envelope.
//!
//! Two layers of proof:
//!
//! 1. `snapshot_then_restore_then_snapshot_is_a_fixed_point` — the
//!    snapshot envelope is deterministic across save/restore. Two
//!    successive snapshots taken from a runtime that was just
//!    restored from snapshot bytes must be byte-equal to the original.
//!    Catches any field that fails to round-trip cleanly.
//!
//! 2. `snapshot_then_restore_yields_bit_identical_forward_run` — after
//!    restoring a snapshot, running the machine forward a few frames
//!    produces the same observable state (snapshot bytes) as running
//!    the original forward by the same amount. Catches diagnostic-only
//!    fields that affect behaviour (they shouldn't).
//!
//! Both tests use a blank Kickstart so they're hermetic and run on
//! every `cargo test --workspace`. ROM-backed tests over real
//! Kickstart / Workbench live in the existing diagnostic harnesses
//! and stay there until A.2 promotes them to a boot-invariant suite.
//!
//! Pattern modelled on `runtime-sinclair-zx-spectrum/tests/runtime_48k.rs`.

use std::error::Error;

use emu198x_shell::{
    HostIo, MachineCore, MachineError, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use format_commodore_amiga_adf::ADF_SIZE_DD;
use runtime_commodore_amiga::{AmigaEcsRuntime, AmigaOcsRuntime, Model};

fn blank_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    // Minimal reset vector — supervisor stack at $00080000, PC at the
    // first ROM word. PC instruction is BRA.S * (loop forever), keeping
    // the CPU in a stable state while the chipset ticks around it.
    kickstart[0] = 0x00;
    kickstart[1] = 0x08;
    kickstart[2] = 0x00;
    kickstart[3] = 0x00;
    kickstart[4] = 0x00;
    kickstart[5] = 0xF8;
    kickstart[6] = 0x00;
    kickstart[7] = 0x08;
    kickstart[8] = 0x60;
    kickstart[9] = 0xFE;
    kickstart
}

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

#[test]
fn snapshot_then_restore_then_snapshot_is_a_fixed_point() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;

    // Run a handful of frames so the chipset has non-trivial state
    // (beam counters advanced, CIA timers run, copper has been kicked
    // by the VBL, etc.). The reset-loop CPU stays at $F80008 but
    // everything else ticks.
    let mut host = null_host();
    original.run_until(MachineTime::new(64_000), &mut host)?;

    let snapshot_a = original.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot_a)?;

    let snapshot_b = restored.snapshot()?;

    assert_eq!(
        snapshot_a.len(),
        snapshot_b.len(),
        "snapshot lengths differ — indicates a non-deterministic field"
    );
    assert_eq!(
        snapshot_a, snapshot_b,
        "snapshot bytes differ after round-trip — see lib field list"
    );
    Ok(())
}

#[test]
fn snapshot_then_restore_yields_bit_identical_forward_run() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut host = null_host();
    original.run_until(MachineTime::new(32_000), &mut host)?;

    let snapshot = original.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    // Run both runtimes forward by the same amount of machine time
    // and expect their snapshots to remain byte-equal afterwards.
    let target = original.time().saturating_add(8_000);
    let mut host_a = null_host();
    original.run_until(target, &mut host_a)?;
    let mut host_b = null_host();
    restored.run_until(target, &mut host_b)?;

    let after_original = original.snapshot()?;
    let after_restored = restored.snapshot()?;

    assert_eq!(
        after_original.len(),
        after_restored.len(),
        "post-run snapshot lengths differ — restore drifted"
    );
    assert_eq!(
        after_original, after_restored,
        "post-run snapshot bytes differ — restore is not bit-equivalent"
    );
    Ok(())
}

#[test]
fn a2000_fat_agnus_snapshot_round_trips_extension_state() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    assert!(original.machine().uses_fat_agnus_8372a());

    // Populate wrapper-only state rather than proving only that the inner
    // OCS Agnus serializes. HTOTAL/VTOTAL/BEAMCON0 drive the concrete ECS
    // clock path; BLTSIZV remains sticky for a later BLTSIZH start.
    original.machine_mut().poke_word(0x00DF_F1C0, 3);
    original.machine_mut().poke_word(0x00DF_F1C8, 1);
    original.machine_mut().poke_word(0x00DF_F1DC, 0x00A0);
    original.machine_mut().poke_word(0x00DF_F05C, 2);
    let mut host = null_host();
    original.run_until(MachineTime::new(64), &mut host)?;

    let snapshot = original.snapshot()?;
    let mut restored = AmigaOcsRuntime::new(Model::A2000OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    assert!(restored.machine().uses_fat_agnus_8372a());
    assert_eq!(restored.machine().read_word(0x00DF_F07C), 0xFFFF);
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "Fat Agnus wrapper state must be byte-stable through postcard"
    );

    let target = original.time().saturating_add(64);
    let mut host_a = null_host();
    original.run_until(target, &mut host_a)?;
    let mut host_b = null_host();
    restored.run_until(target, &mut host_b)?;
    assert_eq!(
        original.snapshot()?,
        restored.snapshot()?,
        "programmed Fat Agnus timing must remain deterministic after restore"
    );
    Ok(())
}

#[test]
fn ecs_vertical_diw_latch_survives_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let mut original = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    original.machine_mut().poke_word(0x00DF_F090, 0x10C1);
    original.machine_mut().poke_word(0x00DF_F08E, 0x0081);
    original.machine_mut().poke_word(0x00DF_F1E4, 0x0000);
    original.machine_mut().poke_word(0x00DF_F1DC, 0x00A0);
    assert!(
        original.machine().agnus_ecs().vertical_diw_active(),
        "a line-zero VSTART comparator should open the vertical-DIW latch",
    );

    let snapshot = original.snapshot()?;
    let mut restored = AmigaEcsRuntime::new(Model::A500PlusEcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;

    assert!(restored.machine().agnus_ecs().vertical_diw_active());
    assert_eq!(
        snapshot,
        restored.snapshot()?,
        "the hidden vertical-DIW latch must be byte-stable through postcard",
    );
    Ok(())
}

#[test]
fn restore_rejects_wrong_model() -> Result<(), Box<dyn Error>> {
    let original = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let snapshot = original.snapshot()?;

    let mut other_model = AmigaOcsRuntime::new(Model::A500OcsPalA501, blank_kickstart())?;
    let result = other_model.restore(&snapshot);
    assert!(result.is_err(), "restoring across models should fail");
    Ok(())
}

#[test]
fn restore_rejects_unknown_version() -> Result<(), Box<dyn Error>> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    // Crafted bytes that won't deserialize as the current envelope — postcard rejects
    // mismatched length / shape and the restore returns an error.
    let result = runtime.restore(&[0xFFu8; 4]);
    assert!(result.is_err(), "garbage bytes should not restore");
    Ok(())
}

/// Take a real snapshot, hand-patch the leading postcard varint version
/// field back to 5, and confirm the version-mismatch arm fires with a
/// human-readable reason naming the snapshot version. The first byte
/// of a `SnapshotEnvelopeV6` is the postcard varint encoding of
/// `version`; for `SNAPSHOT_VERSION = 6` that byte is `0x06`.
/// Replacing it with another single-byte value keeps the envelope
/// length stable and lands us inside the explicit version-mismatch
/// branch (rather than the postcard-parse-error branch above).
#[test]
fn restore_rejects_mismatched_snapshot_version() -> Result<(), Box<dyn Error>> {
    let runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let mut bytes = runtime.snapshot()?;
    assert_eq!(
        bytes[0], 6,
        "postcard varint for SNAPSHOT_VERSION = 6 should be 0x06"
    );
    bytes[0] = 5;

    let mut other = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let err = other
        .restore(&bytes)
        .expect_err("version-5 snapshot should be rejected before payload decode");
    assert!(
        matches!(err, MachineError::InvalidSnapshot { ref reason } if reason.contains("version")),
        "expected version-mismatch reason, got {err:?}"
    );
    Ok(())
}

/// Snapshot taken with an ADF inserted into DF0 round-trips through
/// restore — the `Some(bytes)` arm of `decode` re-mounts the disk via
/// `insert_floppy_bytes_pub`. Without this test the floppy0 re-insert
/// path stays uncovered.
#[test]
fn restore_remounts_persisted_floppy_image() -> Result<(), Box<dyn Error>> {
    let mut runtime = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    let disk = vec![0u8; ADF_SIZE_DD];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime.load_media(&media)?;
    assert!(runtime.machine().drive().has_disk());

    let snapshot = runtime.snapshot()?;

    let mut restored = AmigaOcsRuntime::new(Model::A500OcsPal, blank_kickstart())?;
    restored.restore(&snapshot)?;
    assert!(
        restored.machine().drive().has_disk(),
        "restore should re-mount the persisted disk image"
    );
    Ok(())
}
