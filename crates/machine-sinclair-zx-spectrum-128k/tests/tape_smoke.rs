//! Smoke tests for 128K-specific ULA / timing TAPs.
//!
//! Same shape as the 48K equivalent
//! (`machine-sinclair-zx-spectrum-48k/tests/tape_smoke.rs`):
//! boot, press ENTER on the 128K firmware menu to select the Tape
//! Loader, play the TAP, run the budget, save a PNG, and assert the
//! result screen has rendered enough non-zero attribute variety to
//! constitute "the test ran." Strict per-TAP PNG comparison against
//! Spectron's `tests/Results/<name>_128.png` references is a
//! follow-up.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

const ROM0_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM0";
const ROM1_PATH_ENV: &str = "EMU198X_SPECTRUM_128K_ROM1";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";

const BOOT_FRAMES: usize = 200;
const RUN_BUDGET_FRAMES: usize = 5_000;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
}

fn rom0_path() -> PathBuf {
    std::env::var_os(ROM0_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-128k/128-0.rom"))
}

fn rom1_path() -> PathBuf {
    std::env::var_os(ROM1_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x/roms/sinclair-zx-spectrum-128k/128-1.rom"))
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

/// 128K-class keyboard matrix is a bare `[u8; 8]` (active-low rows),
/// not a `KeyboardMatrix` wrapper.
fn set_key(keyboard: &mut [u8; 8], key: SpectrumKey, pressed: bool) {
    let (row, bit) = key.row_bit();
    let mask = 1u8 << bit;
    if pressed {
        keyboard[row] &= !mask;
    } else {
        keyboard[row] |= mask;
    }
}

/// Press ENTER at the 128K firmware boot menu (Tape Loader is the
/// default-highlighted option), wait long enough for the firmware
/// to switch to ROM 1 (48 BASIC) before the tape starts.
fn press_enter(machine: &mut Spectrum128K) {
    set_key(&mut machine.keyboard, SpectrumKey::Enter, true);
    for _ in 0..6 {
        machine.run_frame();
    }
    set_key(&mut machine.keyboard, SpectrumKey::Enter, false);
    for _ in 0..30 {
        machine.run_frame();
    }
}

fn save_framebuffer_png(machine: &Spectrum128K, path: &Path) -> std::io::Result<()> {
    let framebuffer = &machine.framebuffer;
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

fn run_tap_smoke(tap_filename: &str, png_stem: &str) -> Option<(u32, PathBuf)> {
    let rom0_path = rom0_path();
    let rom1_path = rom1_path();
    if !rom0_path.is_file() || !rom1_path.is_file() {
        eprintln!("128K ROMs not found — skipping");
        return None;
    }
    let tap_path = system_tests_dir().join(tap_filename);
    if !tap_path.is_file() {
        eprintln!("{tap_filename} not found at {} — skipping", tap_path.display());
        return None;
    }

    let tap_bytes = std::fs::read(&tap_path).unwrap_or_else(|e| panic!("{tap_filename}: {e}"));
    let tap_blocks = parse_tap(&tap_bytes).unwrap_or_else(|e| panic!("{tap_filename} parse: {e}"));
    let tape_blocks = tap_blocks_to_tape_blocks(tap_blocks);

    let mut machine = Spectrum128K::new();
    machine.memory.load_rom0(&rom0_path).expect("128 ROM 0");
    machine.memory.load_rom1(&rom1_path).expect("48 ROM 1");
    machine.load_tape_blocks(tape_blocks);

    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }
    press_enter(&mut machine);
    machine.tape_play();

    for _ in 0..RUN_BUDGET_FRAMES {
        machine.run_frame();
    }

    let png_path = std::env::temp_dir().join(format!("{png_stem}.png"));
    save_framebuffer_png(&machine, &png_path).expect("framebuffer should encode as PNG");
    eprintln!("Framebuffer screenshot: {}", png_path.display());

    let mut seen = [false; 256];
    for addr in 0x5800u16..=0x5AFF {
        let v = machine.memory.read(addr);
        seen[v as usize] = true;
    }
    let distinct = seen.iter().filter(|s| **s).count() as u32;

    Some((distinct, png_path))
}

fn assert_test_ran(tap_filename: &str, png_stem: &str) {
    let Some((distinct, png_path)) = run_tap_smoke(tap_filename, png_stem) else {
        return;
    };
    assert!(
        distinct >= 2,
        "{tap_filename}: screen attributes show {distinct} distinct values \
         (a blank screen is 1; a test-result screen is typically 3+). \
         Screenshot at {} — inspect to triage whether the load chain ran.",
        png_path.display(),
    );
}

#[test]
#[ignore = "requires local 128K ROMs and halt2int128.tap; ~100 s wall time"]
fn halt2int128_runs_to_completion() {
    assert_test_ran("halt2int128.tap", "halt2int128");
}

#[test]
#[ignore = "requires local 128K ROMs and Super HALT Invaders TAP; ~120 s wall time"]
fn super_halt_invaders_runs_to_completion() {
    assert_test_ran(
        "Super HALT Invaders Test (2021-10-07)(Woodmass, Mark)[!].tap",
        "super-halt-invaders",
    );
}
