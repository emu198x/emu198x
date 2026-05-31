//! Golden-screenshot tests for the ULA / contention / timing TAPs
//! that ship with Spectron and Mark Woodmass's Super HALT Invaders
//! set.
//!
//! Each test loads its TAP via the cycle-accurate tape pipeline
//! (boot, type `LOAD ""`, play tape, run to quiescent), encodes the
//! resulting 352×296 paletted framebuffer through `SPECTRUM_PALETTE`
//! as a 16-colour indexed PNG, and either writes it as a new golden
//! (when `UPDATE_GOLDENS=1` is set or the golden is missing) or
//! compares decoded bytes against the checked-in golden in
//! `tests/goldens/`. Same shape as `runtime-sinclair-zx-spectrum`'s
//! `goldens.rs` for the boot screens.
//!
//! Strict byte-equal comparison against Spectron's
//! `tests/Results/<name>_48.png` references is impractical (Spectron
//! renders at 1224×968 with border + scaling), but locking our own
//! goldens still gives a regression contract over every cycle-counted
//! line of the result frame — drift in tape timing, BASIC interpreter
//! cycle budget, ULA contention, or the Z80's bus probe all show up
//! as a pixel diff.
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
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
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

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// Spectrum palette flattened to RGB bytes for PNG indexed colour mode.
fn palette_rgb() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SPECTRUM_PALETTE.len() * 3);
    for entry in &SPECTRUM_PALETTE {
        let r = ((entry >> 24) & 0xFF) as u8;
        let g = ((entry >> 16) & 0xFF) as u8;
        let b = ((entry >> 8) & 0xFF) as u8;
        bytes.extend_from_slice(&[r, g, b]);
    }
    bytes
}

fn write_indexed_png(path: &Path, framebuffer: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create goldens dir");
    }
    let file = std::fs::File::create(path).expect("create golden file");
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette_rgb());
    let mut writer = encoder.write_header().expect("write png header");
    writer
        .write_image_data(framebuffer)
        .expect("write png image data");
}

fn read_indexed_png(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("decode png header");
    let mut buf = vec![
        0u8;
        reader
            .output_buffer_size()
            .expect("png buffer size fits in usize")
    ];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    buf.truncate(info.buffer_size());
    assert_eq!(
        info.color_type,
        png::ColorType::Indexed,
        "golden {} should be indexed PNG",
        path.display()
    );
    assert_eq!(
        (info.width as usize, info.height as usize),
        (SCREEN_WIDTH, SCREEN_HEIGHT),
        "golden {} dimensions {}×{} don't match expected {}×{}",
        path.display(),
        info.width,
        info.height,
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
    );
    buf
}

fn compare_or_update(test_name: &str, framebuffer: &[u8]) {
    let path = goldens_dir().join(format!("{test_name}.png"));
    let updating = std::env::var_os("UPDATE_GOLDENS").is_some();
    let missing = !path.exists();

    if updating || missing {
        write_indexed_png(&path, framebuffer);
        if missing && !updating {
            panic!(
                "golden {} did not exist — wrote it now, re-run to verify",
                path.display()
            );
        }
        return;
    }

    let expected = read_indexed_png(&path);
    if expected == framebuffer {
        return;
    }
    let differing = expected
        .iter()
        .zip(framebuffer.iter())
        .filter(|(a, b)| a != b)
        .count();
    let live_path = std::env::temp_dir().join(format!("{test_name}-live.png"));
    write_indexed_png(&live_path, framebuffer);
    panic!(
        "framebuffer for {test_name} differs from {} ({differing} of {} pixels). \
         Live frame written to {} for visual diff. \
         Re-run with UPDATE_GOLDENS=1 to refresh after eyeballing the change.",
        path.display(),
        framebuffer.len(),
        live_path.display(),
    );
}

/// Common runner: load TAP, boot, type LOAD"", play tape, run for the
/// budget, compare framebuffer to the locked golden.
fn run_and_compare(test_name: &str) {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        eprintln!("48K ROM not found at {} — skipping", rom_path.display());
        return;
    }
    let tap_path = system_tests_dir().join(format!("{test_name}.tap"));
    if !tap_path.is_file() {
        eprintln!(
            "{}.tap not found at {} — skipping",
            test_name,
            tap_path.display()
        );
        return;
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

    compare_or_update(test_name, machine.framebuffer());
}

#[test]
#[ignore = "requires local 48K ROM and floatspy.tap; ~100 s wall time at cycle-accurate tape speed"]
fn floatspy_runs_to_completion() {
    run_and_compare("floatspy");
}

#[test]
#[ignore = "requires local 48K ROM and halt2int.tap; ~100 s wall time"]
fn halt2int_runs_to_completion() {
    run_and_compare("halt2int");
}

#[test]
#[ignore = "requires local 48K ROM and btime.tap; ~100 s wall time"]
fn btime_runs_to_completion() {
    run_and_compare("btime");
}

#[test]
#[ignore = "requires local 48K ROM and ptime.tap; ~100 s wall time"]
fn ptime_runs_to_completion() {
    run_and_compare("ptime");
}

// Super HALT Invaders Test is 128K-only; see the 128K crate's
// `tape_smoke.rs` for its test wiring.
