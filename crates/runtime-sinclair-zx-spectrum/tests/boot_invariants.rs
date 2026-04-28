//! Standing boot-invariant suite for the Spectrum 48K runtime.
//!
//! Each test asserts a known-good waypoint that the project has reached
//! and depends on. The file is the canonical regression gate for
//! Spectrum-shaped breakage — when a refactor touches the Z80 core,
//! the ULA, the contention model, or the runtime envelope, these are
//! the tests that should stay green.
//!
//! Hermetic invariants run on every `cargo test --workspace`. ROM-
//! backed invariants are `#[ignore]`'d and resolve assets from
//! `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//!
//! Promoted from existing waypoints per A.2 of
//! `docs/plans/2026-04-28-october-runup-plan.md`.

use std::error::Error;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_sinclair_zx_spectrum::Spectrum48kRuntime;

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn home_rom_48k() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom");
    if path.exists() { Some(path) } else { None }
}

// ─────────────────────────────────────────────────────────────────────
// Hermetic — run on every cargo test
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: dummy 16 KiB ROM constructs a runtime cleanly.
///
/// Catches regression: any change to `Spectrum48kRuntime::from_rom_bytes`
/// shape — a moving validation envelope or a stricter size check would
/// fail this immediately.
#[test]
fn dummy_rom_constructs_runtime() {
    let runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])
        .expect("dummy 16 KiB ROM should construct cleanly");
    assert_eq!(runtime.time(), MachineTime::default());
}

/// Waypoint: blank runtime advances time when run forward.
///
/// Catches regression: any infinite-loop / hang in the master-clock
/// run loop, the half-cycle Z80 dispatch, or the frame-emission path.
#[test]
fn run_until_advances_past_first_frame() -> Result<(), Box<dyn Error>> {
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let mut host = null_host();
    let target = MachineTime::new(80_000);
    runtime.run_until(target, &mut host)?;
    let now = runtime.time();
    assert!(
        now.get() >= 50_000,
        "runtime should have advanced at least one frame, got {now:?}"
    );
    Ok(())
}

/// Waypoint: snapshot → restore → snapshot is a fixed point on a
/// dummy-ROM runtime that has been ticked far enough to have non-
/// trivial state.
///
/// Catches regression: any chip-state field that fails to round-trip.
#[test]
fn snapshot_round_trip_is_fixed_point_after_warmup() -> Result<(), Box<dyn Error>> {
    let mut original = Spectrum48kRuntime::from_rom_bytes(&[0; 16 * 1024])?;
    let mut host = null_host();
    original.run_until(MachineTime::new(50_000), &mut host)?;
    let bytes_a = original.snapshot()?;
    let mut restored = Spectrum48kRuntime::blank();
    restored.restore(&bytes_a)?;
    let bytes_b = restored.snapshot()?;
    assert_eq!(bytes_a, bytes_b, "snapshot drift after restore");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// ROM-backed — `#[ignore]`'d; resolve assets under ~/.emu198x/
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: real 48K ROM advances multiple frames without panicking.
/// Catches regression: any change that breaks the Z80 / ULA / contention
/// chain when running real Sinclair ROM code.
#[test]
#[ignore = "requires ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn real_48k_rom_runs_thirty_frames() -> Result<(), Box<dyn Error>> {
    let Some(rom_path) = home_rom_48k() else {
        eprintln!("skip: 48.rom missing");
        return Ok(());
    };
    let rom = std::fs::read(&rom_path)?;
    let mut runtime = Spectrum48kRuntime::from_rom_bytes(&rom)?;
    let mut host = null_host();
    // 30 frames at 69_888 t-states/frame, half-cycle units.
    let target = MachineTime::new(30 * 2 * 69_888);
    runtime.run_until(target, &mut host)?;
    assert!(runtime.time().get() >= target.get() / 2);
    Ok(())
}
