//! Standing boot-invariant suite for the C64 runtime.
//!
//! Each test asserts a known-good waypoint that the project has reached
//! and depends on. The file is the canonical regression gate for
//! C64-shaped breakage — when a refactor touches the 6502 core, the
//! VIC-II / SID / CIA chain, the 1541 path, or the runtime envelope,
//! these are the tests that should stay green.
//!
//! Hermetic invariants run on every `cargo test --workspace`. ROM-
//! backed invariants are `#[ignore]`'d and resolve assets from
//! `~/.emu198x/roms/commodore-c64/`.
//!
//! Promoted from existing waypoints per A.2 of
//! `docs/plans/2026-04-28-october-runup-plan.md`.

use std::error::Error;
use std::path::PathBuf;

use emu198x_shell::{
    HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_commodore_c64::{C64Runtime, Model};

const KERNAL_SIZE: usize = 0x2000;
const BASIC_SIZE: usize = 0x2000;
const CHARGEN_SIZE: usize = 0x1000;

fn null_host() -> HostIo<'static> {
    HostIo {
        input_events: &[],
        frame_sink: Box::leak(Box::new(NullFrameSink)),
        audio_sink: Box::leak(Box::new(NullAudioSink)),
        trace_sink: Box::leak(Box::new(NullTraceSink)),
    }
}

fn home_c64_rom_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-c64");
    if path.exists() { Some(path) } else { None }
}

// ─────────────────────────────────────────────────────────────────────
// Hermetic — run on every cargo test
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: dummy KERNAL/BASIC/CHARGEN ROMs construct a runtime cleanly.
///
/// Catches regression: any ROM-validation envelope change. The fixed
/// 8 KiB / 8 KiB / 4 KiB sizes are part of the C64 model contract.
#[test]
fn dummy_roms_construct_runtime() {
    let runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_SIZE],
        vec![0; BASIC_SIZE],
        vec![0; CHARGEN_SIZE],
        None,
    )
    .expect("dummy C64 ROM set should construct cleanly");
    assert_eq!(runtime.time(), MachineTime::default());
}

/// Waypoint: blank runtime ticks past the first frame boundary.
///
/// Catches regression: any infinite-loop / hang in the master-clock
/// run loop, the 6502 dispatch, the VIC-II tick, or the frame-emission
/// path.
#[test]
fn run_until_advances_past_first_frame() -> Result<(), Box<dyn Error>> {
    let mut runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_SIZE],
        vec![0; BASIC_SIZE],
        vec![0; CHARGEN_SIZE],
        None,
    )?;
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
    let mut original = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_SIZE],
        vec![0; BASIC_SIZE],
        vec![0; CHARGEN_SIZE],
        None,
    )?;
    let mut host = null_host();
    original.run_until(MachineTime::new(50_000), &mut host)?;
    let bytes_a = original.snapshot()?;
    let mut restored = C64Runtime::blank(Model::C64PalBreadbin);
    restored.restore(&bytes_a)?;
    let bytes_b = restored.snapshot()?;
    assert_eq!(bytes_a, bytes_b, "snapshot drift after restore");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// ROM-backed — `#[ignore]`'d; resolve assets under ~/.emu198x/
// ─────────────────────────────────────────────────────────────────────

/// Waypoint: real KERNAL/BASIC/CHARGEN reaches the `READY.` prompt.
/// Catches regression: every load-bearing 6502 / VIC-II / CIA / memory
/// banking change. This is the C64 equivalent of the Spectrum 48K
/// ©1982 boot screen.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-c64/{kernal,basic,chargen}.rom"]
fn real_kernal_reaches_ready_prompt() -> Result<(), Box<dyn Error>> {
    let Some(rom_dir) = home_c64_rom_dir() else {
        eprintln!("skip: no C64 ROM dir");
        return Ok(());
    };
    let kernal = std::fs::read(rom_dir.join("kernal.rom"))?;
    let basic = std::fs::read(rom_dir.join("basic.rom"))?;
    let chargen = std::fs::read(rom_dir.join("chargen.rom"))?;

    let mut runtime = C64Runtime::new(Model::C64PalBreadbin, kernal, basic, chargen, None)?;
    let mut host = null_host();

    // 200 PAL frames at ~50Hz is ~4 seconds — KERNAL reaches READY in
    // about 2.5s on real hardware.
    let pal_frame_ticks: u64 = 985_248 / 50;
    runtime.run_until(MachineTime::new(200 * pal_frame_ticks), &mut host)?;

    let machine = runtime.machine();
    // Screen codes for "READY." in the screen RAM at $0400-$07E7.
    const READY: [u8; 6] = [18, 5, 1, 4, 25, 46];
    let mut found = false;
    for offset in 0..=(0x07E8u16 - 0x0400 - READY.len() as u16) {
        let mut matched = true;
        for (i, &expected) in READY.iter().enumerate() {
            if machine.memory().ram_read(0x0400 + offset + i as u16) != expected {
                matched = false;
                break;
            }
        }
        if matched {
            found = true;
            break;
        }
    }
    assert!(found, "C64 should reach READY. prompt within 200 frames");
    Ok(())
}
