//! Standing boot-invariant suite for the NES runtime.
//!
//! Each test asserts a known-good waypoint that the project has reached
//! and depends on. The file is the canonical regression gate for
//! NES-shaped breakage — when a refactor touches the 2A03 / 2C02
//! interleave, mapper dispatch, the iNES parser, or the runtime
//! envelope, these are the tests that should stay green.
//!
//! Hermetic invariants run on every `cargo test --workspace`. ROM-
//! backed invariants are `#[ignore]`'d and resolve assets from the
//! Tom Harte / nestest fixture archives.
//!
//! Promoted from existing waypoints per A.2 of
//! `docs/plans/2026-04-28-october-runup-plan.md`.

use std::error::Error;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_nintendo_nes::{Model, NesRuntime};

const NTSC_FRAME_TICKS: u64 = 341 * 262;

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn minimal_ines() -> Vec<u8> {
    // 16 KiB PRG of NOP (0xEA) instructions with the reset vector
    // pointing at $8000 — the CPU keeps NOPing forever, the PPU runs
    // alongside, the APU mixer ticks. Mirrors `minimal_ines` from the
    // runtime crate's unit tests so the hermetic invariants exercise
    // the same path the runtime tests do.
    let mut prg = vec![0xeau8; 16 * 1024];
    prg[0x3ffc] = 0x00;
    prg[0x3ffd] = 0x80;
    let chr = vec![0u8; 8 * 1024];
    let mut data = vec![0u8; 16 + prg.len() + chr.len()];
    data[0..4].copy_from_slice(b"NES\x1a");
    data[4] = 1; // 1 × 16 KiB PRG
    data[5] = 1; // 1 × 8 KiB CHR
    data[16..16 + prg.len()].copy_from_slice(&prg);
    data[16 + prg.len()..].copy_from_slice(&chr);
    data
}

fn nestest_fixture_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NES_TEST_DATA") {
        let d = PathBuf::from(p);
        if d.join("nestest.nes").exists() {
            return Some(d);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let d = PathBuf::from(home)
        .join("Projects/Emu198x-Unclean/Reference/nintendo/nes/test-suites/other");
    if d.join("nestest.nes").exists() {
        Some(d)
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────
// Hermetic — run on every cargo test
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: minimal-iNES NROM cartridge loads through the runtime
/// load-media path.
///
/// Catches regression: any change to the iNES parser, NROM mapper, or
/// `MediaKind::Cartridge` routing.
#[test]
fn minimal_ines_cartridge_loads() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    assert!(
        runtime.machine().is_some(),
        "minimal iNES should produce a live machine"
    );
    Ok(())
}

/// Waypoint: minimal-iNES NROM runs one full frame and produces frame
/// count = 1.
///
/// Catches regression: any infinite-loop / hang in the master-clock
/// run loop, the 1:3 CPU:PPU interleave, NMI sampling, or the frame-
/// emission path.
#[test]
fn minimal_ines_runs_one_frame() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    let mut host = null_host();
    runtime.run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)?;
    let frames = runtime.machine().expect("cartridge loaded").frame_count();
    assert_eq!(frames, 1, "exactly one frame should have completed");
    Ok(())
}

/// Waypoint: snapshot → restore → snapshot is a fixed point on a
/// minimal-iNES runtime that has been ticked through a frame.
///
/// Catches regression: any chip-state field that fails to round-trip
/// through the NES runtime envelope.
#[test]
fn snapshot_round_trip_is_fixed_point_after_one_frame() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut original = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    original.load_media(&media)?;
    let mut host = null_host();
    original.run_until(MachineTime::new(NTSC_FRAME_TICKS), &mut host)?;

    let bytes_a = original.snapshot()?;
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored.restore(&bytes_a)?;
    let bytes_b = restored.snapshot()?;
    assert_eq!(bytes_a, bytes_b, "snapshot drift after restore");
    Ok(())
}

/// Seam 5 waypoint: NES snapshot envelope version is locked at 1.
///
/// Postcard varint-encodes the leading `version: u16` field as a
/// single byte (for value ≤ 127). A silent bump would change the
/// first byte and break replay compatibility with previously-captured
/// snapshots. Catches a regression where someone bumps the constant
/// without an explicit decision.
#[test]
fn snapshot_envelope_version_is_locked_at_v1() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    let bytes = runtime.snapshot()?;
    assert!(!bytes.is_empty(), "snapshot must have a non-empty envelope");
    assert_eq!(
        bytes[0], 1,
        "NES snapshot envelope version should be 1 (got {})",
        bytes[0]
    );
    Ok(())
}

/// Seam 5 waypoint: controller 1 + 2 state survives the snapshot
/// round-trip. Two-player games rely on both controllers' shift
/// register / strobe state restoring cleanly after a state-load.
///
/// Catches regression: forgetting to wire controller_2_state through
/// NesSnapshot (which we just fixed in Seam 2), or a future refactor
/// dropping one of the fields.
#[test]
fn both_controllers_survive_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut original = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    original.load_media(&media)?;

    // Set distinct controller states. Picks specific patterns so any
    // crosstalk (e.g. controller 1 state landing on controller 2) is
    // visible.
    let machine = original.machine_mut().expect("cartridge loaded");
    machine.set_controller1(0b0101_0101);
    machine.set_controller2(0b1010_1010);

    let bytes = original.snapshot()?;
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored.restore(&bytes)?;
    let restored_machine = restored.machine().expect("restored cartridge");
    assert_eq!(
        restored_machine.controller1_state, 0b0101_0101,
        "controller 1 state must survive round-trip"
    );
    assert_eq!(
        restored_machine.controller2_state, 0b1010_1010,
        "controller 2 state must survive round-trip"
    );
    Ok(())
}

/// Seam 5 waypoint: cartridge bytes survive snapshot round-trip. The
/// runtime caches the raw iNES bytes alongside the chip snapshot so
/// state-loads can recreate the mapper without re-reading the
/// original file.
///
/// Catches regression: any refactor that drops the cartridge-bytes
/// cache from the envelope would break replay after a state load.
/// Detection strategy: the iNES magic "NES\x1A" is at offset 0 of
/// every iNES file. The snapshot envelope is postcard-encoded, so
/// the cartridge bytes appear as a sub-slice somewhere inside; we
/// scan for the magic to confirm survival without exposing internal
/// runtime accessors.
#[test]
fn cartridge_bytes_survive_snapshot_round_trip() -> Result<(), Box<dyn Error>> {
    let rom = minimal_ines();
    let mut original = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    original.load_media(&media)?;

    let bytes = original.snapshot()?;

    // Confirm the iNES magic survives — proves the cartridge buffer
    // is in the snapshot envelope (not dropped to None).
    let magic = b"NES\x1a";
    let magic_present = bytes.windows(magic.len()).any(|w| w == magic);
    assert!(
        magic_present,
        "iNES magic should appear in the snapshot envelope"
    );

    // The restored runtime should also produce a snapshot containing
    // the magic, proving round-trip survival.
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored.restore(&bytes)?;
    let restored_bytes = restored.snapshot()?;
    let magic_present_after = restored_bytes.windows(magic.len()).any(|w| w == magic);
    assert!(
        magic_present_after,
        "iNES magic should survive snapshot → restore → snapshot"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// ROM-backed — `#[ignore]`'d
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: nestest.nes runs at least one frame through the runtime
/// without blowing up. The detailed 8991/8991 register-comparison test
/// stays in `machine-nintendo-nes::tests::nestest`; this is the
/// runtime-layer equivalent that catches load-media-path regressions.
#[test]
#[ignore = "requires nestest.nes — set NES_TEST_DATA or place under ~/Projects/Emu198x-Unclean/..."]
fn nestest_loads_and_runs() -> Result<(), Box<dyn Error>> {
    let Some(dir) = nestest_fixture_dir() else {
        emu198x_test_skip::record("skip: nestest.nes missing");
        return Ok(());
    };
    let rom = std::fs::read(dir.join("nestest.nes"))?;
    let mut runtime = NesRuntime::blank(Model::NesNtsc);
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    let mut host = null_host();
    runtime.run_until(MachineTime::new(NTSC_FRAME_TICKS * 5), &mut host)?;
    let frames = runtime.machine().expect("cartridge loaded").frame_count();
    assert!(frames >= 5, "nestest should run at least five frames");
    Ok(())
}
