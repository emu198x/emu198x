//! Smoke tests for ULA / contention / timing TAPs that ship with
//! Spectron and Mark Woodmass's Super HALT Invaders set.
//!
//! Each test loads its TAP via the cycle-accurate tape pipeline
//! (boot, type `LOAD ""`, play tape, run until quiescent), saves a
//! framebuffer screenshot under `$TMPDIR`, and asserts the screen
//! has rendered enough non-zero content to constitute "test ran
//! to completion." This is a deliberately weak signal — it catches
//! crashes, boot regressions, tape-loader breakage, and the load
//! chain itself, but not finer-grained timing correctness.
//!
//! Strict per-test PNG comparison against Spectron's
//! `tests/Results/<name>_48.png` references is a follow-up; the
//! reference PNGs live at
//! `~/Projects/198x/emulators/zx-spectrum/Spectron/tests/Results/`.
//!
//! Required local fixtures (resolved in this order):
//!
//! - `$EMU198X_SPECTRUM_48K_ROM`, defaulting to
//!   `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
//! - `$EMU198X_SPECTRUM_SYSTEM_TESTS_DIR/<name>.tap`, defaulting
//!   to `~/.emu198x/test-data/spectrum-system-tests/<name>.tap`.
//!
//! Skipped (returning `ok`) when fixtures are missing so CI without
//! local data stays green.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";

const BOOT_FRAMES: usize = 200;
const RUN_BUDGET_FRAMES: usize = 5_000;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
}

fn rom_path() -> PathBuf {
    std::env::var_os(ROM_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
}

fn system_tests_dir() -> PathBuf {
    std::env::var_os(SYSTEM_TESTS_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/test-data/spectrum-system-tests"))
}

fn tap_blocks_to_tape_blocks(blocks: Vec<TapBlock>) -> Vec<TapeBlock> {
    blocks
        .into_iter()
        .map(|block| {
            let mut full = Vec::with_capacity(block.data.len() + 2);
            full.push(block.flag);
            full.extend_from_slice(&block.data);
            let checksum = full.iter().fold(0u8, |acc, &byte| acc ^ byte);
            full.push(checksum);
            TapeBlock {
                flag: block.flag,
                data: full,
            }
        })
        .collect()
}

/// Type `LOAD ""<ENTER>` at the BASIC command prompt. At the K cursor
/// `J` emits the LOAD keyword; `SS+P` emits `"`; ENTER terminates.
fn type_load_command(machine: &mut Spectrum48k, start_frame: usize) -> usize {
    let mut frame = start_frame;
    let press = |m: &mut Spectrum48k, k: SpectrumKey, on: bool| m.keyboard_mut().set_key(k, on);

    let tap = |m: &mut Spectrum48k, frame: &mut usize, k: SpectrumKey| {
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, true);
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, false);
    };

    let chord = |m: &mut Spectrum48k, frame: &mut usize, mo: SpectrumKey, k: SpectrumKey| {
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, mo, true);
        press(m, k, true);
        for _ in 0..6 {
            *frame += 1;
            m.run_frame();
        }
        press(m, k, false);
        press(m, mo, false);
    };

    tap(machine, &mut frame, SpectrumKey::J);
    chord(machine, &mut frame, SpectrumKey::SymbolShift, SpectrumKey::P);
    chord(machine, &mut frame, SpectrumKey::SymbolShift, SpectrumKey::P);
    tap(machine, &mut frame, SpectrumKey::Enter);
    frame
}

/// Save the current framebuffer as an RGBA PNG using the Spectrum
/// palette.
fn save_framebuffer_png(machine: &Spectrum48k, path: &Path) -> std::io::Result<()> {
    let framebuffer = machine.framebuffer();
    assert_eq!(framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);

    let mut rgba = Vec::with_capacity(SCREEN_WIDTH * SCREEN_HEIGHT * 4);
    for &idx in framebuffer {
        let rgb = SPECTRUM_PALETTE[(idx as usize) & 0x0F];
        rgba.push(((rgb >> 16) & 0xFF) as u8);
        rgba.push(((rgb >> 8) & 0xFF) as u8);
        rgba.push((rgb & 0xFF) as u8);
        rgba.push(0xFF);
    }

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

/// Common smoke runner: load TAP, boot, type LOAD"", play tape, run
/// for the budget, save PNG. Returns the count of distinct attribute
/// values in the screen RAM (a rough "test wrote results to screen"
/// signal that beats simple nonzero count for test ROMs that paint a
/// full coloured screen).
fn run_tap_smoke(test_name: &str) -> Option<(u32, PathBuf)> {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        eprintln!("48K ROM not found at {} — skipping", rom_path.display());
        return None;
    }
    let tap_path = system_tests_dir().join(format!("{test_name}.tap"));
    if !tap_path.is_file() {
        eprintln!(
            "{}.tap not found at {} — skipping",
            test_name,
            tap_path.display()
        );
        return None;
    }

    let rom = std::fs::read(&rom_path).expect("48K ROM should read");
    let tap_bytes = std::fs::read(&tap_path).unwrap_or_else(|e| panic!("{test_name}.tap: {e}"));
    let tap_blocks = parse_tap(&tap_bytes).unwrap_or_else(|e| panic!("{test_name}.tap parse: {e}"));
    let tape_blocks = tap_blocks_to_tape_blocks(tap_blocks);

    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("48K ROM should load");
    machine.reset();
    machine.load_tape_blocks(tape_blocks);

    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }
    let after_typing = type_load_command(&mut machine, BOOT_FRAMES);
    for _ in 0..30 {
        machine.run_frame();
    }
    machine.play_tape();

    for _ in (after_typing + 30)..(after_typing + 30 + RUN_BUDGET_FRAMES) {
        machine.run_frame();
    }

    let png_path = std::env::temp_dir().join(format!("{test_name}.png"));
    save_framebuffer_png(&machine, &png_path).expect("framebuffer should encode as PNG");
    eprintln!("Framebuffer screenshot: {}", png_path.display());

    // Count distinct attribute bytes in screen RAM attribute area
    // ($5800-$5AFF, 768 bytes). A blank-screen Spectrum has 1 distinct
    // value; a test-result screen typically has 3+ (border, panels,
    // text colours).
    let mut seen = [false; 256];
    for addr in 0x5800u16..=0x5AFF {
        let v = machine.read(addr);
        seen[v as usize] = true;
    }
    let distinct = seen.iter().filter(|s| **s).count() as u32;

    Some((distinct, png_path))
}

fn assert_test_ran(test_name: &str) {
    let Some((distinct, png_path)) = run_tap_smoke(test_name) else {
        return;
    };
    assert!(
        distinct >= 2,
        "{test_name}: screen attributes show {distinct} distinct values \
         (a blank screen is 1; a test-result screen is typically 3+). \
         Screenshot at {} — inspect to triage whether the load chain ran.",
        png_path.display(),
    );
}

#[test]
#[ignore = "requires local 48K ROM and floatspy.tap; ~100 s wall time at cycle-accurate tape speed"]
fn floatspy_runs_to_completion() {
    assert_test_ran("floatspy");
}

#[test]
#[ignore = "requires local 48K ROM and halt2int.tap; ~100 s wall time"]
fn halt2int_runs_to_completion() {
    assert_test_ran("halt2int");
}

#[test]
#[ignore = "requires local 48K ROM and btime.tap; ~100 s wall time"]
fn btime_runs_to_completion() {
    assert_test_ran("btime");
}

#[test]
#[ignore = "requires local 48K ROM and ptime.tap; ~100 s wall time"]
fn ptime_runs_to_completion() {
    assert_test_ran("ptime");
}

// Super HALT Invaders Test is 128K-only; see the 128K crate's
// `tape_smoke.rs` for its test wiring.
