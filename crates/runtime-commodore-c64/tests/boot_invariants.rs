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

/// Seam 5 waypoint: C64 snapshot envelope version is locked at 8.
///
/// Postcard varint-encodes the leading `version: u32` field as a
/// single byte (for value ≤ 127). A silent bump would change the
/// first byte and break replay compatibility with previously-captured
/// snapshots. Catches a regression where someone bumps the constant
/// without an explicit decision. Bumped to 2 when the fixed
/// 1541-plus-1581 pair became a per-port drive array (devices 8–11), and
/// to 3 when the runtime-level expansion bookkeeping (cartridge image,
/// GeoRAM/REU sizes, 1351-mouse port) joined the envelope so a restored
/// snapshot keeps them across a reset, and to 4 when live VIC-II sprite
/// pipeline state and queued SID samples became part of arbitrary-phase
/// snapshots, to 5 when the global VIC-II BA-to-AEC delay became explicit
/// serialised state, to 6 when source-resolved BA and late-badline window
/// state became inspectable and restorable, and to 7 when the forced-badline
/// C/V/G output-delay and bounded C-data carry became explicit snapshot state,
/// and to 8 when the fixture-specific saved matrix entry became the exact
/// 12-bit C-data carry age and value.
#[test]
fn snapshot_envelope_version_is_locked_at_v8() -> Result<(), Box<dyn Error>> {
    let runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_SIZE],
        vec![0; BASIC_SIZE],
        vec![0; CHARGEN_SIZE],
        None,
    )?;
    let bytes = runtime.snapshot()?;
    assert!(!bytes.is_empty(), "snapshot must have a non-empty envelope");
    assert_eq!(
        bytes[0], 8,
        "C64 snapshot envelope version should be 8 (got {})",
        bytes[0]
    );
    Ok(())
}

/// Seam 5 waypoint: the 6510 `$01` I/O port controls memory banking
/// per the LORAM/HIRAM/CHAREN bits. Changing those bits must
/// re-map the address space: with the default $37 the KERNAL/BASIC
/// ROMs and character ROM are visible; with $30 all RAM shows
/// through.
///
/// Catches regression: any change to the I/O port banking that
/// would silently break every LOAD + RUN. Uses recognisable byte
/// patterns in our dummy ROMs (0x42 for BASIC, 0xEA for KERNAL —
/// the existing stub_machine pattern from the machine crate).
#[test]
fn six510_io_port_banking_changes_active_rom() -> Result<(), Box<dyn Error>> {
    // Build ROMs with distinct fill bytes so we can tell ROM from RAM.
    let mut kernal = vec![0xEAu8; KERNAL_SIZE]; // NOP fills KERNAL
    kernal[0x1FFC] = 0x00; // reset vector
    kernal[0x1FFD] = 0xE0;
    let basic = vec![0x42u8; BASIC_SIZE]; // 'B' fills BASIC

    let mut runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        kernal,
        basic,
        vec![0xCCu8; CHARGEN_SIZE],
        None,
    )?;
    // Tick a few cycles to settle the reset sequence.
    let mut host = null_host();
    runtime.run_until(MachineTime::new(20), &mut host)?;

    let machine = runtime.machine_mut();
    // Default $01 = $37: KERNAL + BASIC + I/O visible at top of RAM.
    // Read $A000 (BASIC ROM start) — should be 0x42.
    assert_eq!(
        machine.cpu_read(0xA000),
        0x42,
        "$A000 with $01=$37 (default) should read BASIC ROM (0x42)"
    );
    // Read $E000 (KERNAL ROM start) — should be 0xEA.
    assert_eq!(
        machine.cpu_read(0xE000),
        0xEA,
        "$E000 with $01=$37 should read KERNAL ROM (0xEA)"
    );

    // Seed the RAM under the ROM windows with known values so the all-RAM
    // read is deterministic regardless of the power-on RAM pattern (writes
    // through a ROM window fall through to the RAM underneath).
    machine.cpu_write(0xA000, 0x11);
    machine.cpu_write(0xE000, 0x22);

    // Switch to $01 = $30: all RAM (no ROMs visible).
    machine.cpu_write(0x0001, 0x30);
    assert_eq!(
        machine.cpu_read(0xA000),
        0x11,
        "$A000 with $01=$30 should read the RAM underneath, not BASIC ROM"
    );
    assert_eq!(
        machine.cpu_read(0xE000),
        0x22,
        "$E000 with $01=$30 should read the RAM underneath, not KERNAL ROM"
    );
    Ok(())
}

/// Seam 5 waypoint: CIA2 PA bits 0-1 (inverted) select the 16 KiB
/// bank the VIC-II sees. Writing different values to $DD00 must
/// change `vic.bank()`. Real silicon: bank 0 = $0000-$3FFF,
/// bank 1 = $4000-$7FFF, bank 2 = $8000-$BFFF (with CHARGEN at
/// $1000-$1FFF), bank 3 = $C000-$FFFF.
///
/// Catches regression: any change that breaks the CIA2 → VIC bank
/// path would garble every game's graphics.
#[test]
fn cia2_pa_drives_vic_bank_select() -> Result<(), Box<dyn Error>> {
    let mut runtime = C64Runtime::new(
        Model::C64PalBreadbin,
        vec![0; KERNAL_SIZE],
        vec![0; BASIC_SIZE],
        vec![0; CHARGEN_SIZE],
        None,
    )?;
    let mut host = null_host();
    runtime.run_until(MachineTime::new(20), &mut host)?;

    let machine = runtime.machine_mut();
    // CIA2 data direction register $DD02 = $FF (all outputs).
    machine.cpu_write(0xDD02, 0xFF);

    // Write $DD00 = $03 (bits 0-1 set). Inverted: VIC bank 0.
    machine.cpu_write(0xDD00, 0x03);
    runtime.run_until(MachineTime::new(40), &mut host)?;
    assert_eq!(runtime.machine().vic_bank(), 0, "CIA2 PA=$03 → VIC bank 0");

    // $DD00 = $02 → inverted → bank 1.
    runtime.machine_mut().cpu_write(0xDD00, 0x02);
    runtime.run_until(MachineTime::new(60), &mut host)?;
    assert_eq!(runtime.machine().vic_bank(), 1, "CIA2 PA=$02 → VIC bank 1");

    // $DD00 = $01 → bank 2.
    runtime.machine_mut().cpu_write(0xDD00, 0x01);
    runtime.run_until(MachineTime::new(80), &mut host)?;
    assert_eq!(runtime.machine().vic_bank(), 2, "CIA2 PA=$01 → VIC bank 2");

    // $DD00 = $00 → bank 3.
    runtime.machine_mut().cpu_write(0xDD00, 0x00);
    runtime.run_until(MachineTime::new(100), &mut host)?;
    assert_eq!(runtime.machine().vic_bank(), 3, "CIA2 PA=$00 → VIC bank 3");
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
        emu198x_test_skip::record("skip: no C64 ROM dir");
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
