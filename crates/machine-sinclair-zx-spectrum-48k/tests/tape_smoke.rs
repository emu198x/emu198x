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
//! Each smoke also byte-compares its 256×192 screen content against
//! Spectron's `tests/Results/<name>_48.png` reference when
//! `EMU198X_SPECTRON_RESULTS_DIR` is set — a trusted timing-accurate
//! oracle, not just our own golden (#10). Spectron's PNGs turn out to
//! be clean 4× nearest-neighbour scales of its raw framebuffer, so
//! after downscaling and mapping both sides to Spectrum colour indices
//! the content compares exactly. `btime` matches byte-for-byte today;
//! `floatspy`/`halt2int` need run-to-completion input driving (their
//! references are the finished self-test screen, our capture the
//! interactive menu) before the compare is meaningful; `ptime` has no
//! 48K reference to compare against. The self-locked golden stays as a
//! second regression contract: drift in tape timing, BASIC interpreter
//! cycle budget, ULA contention, or the Z80's bus probe all show up as
//! a pixel diff.
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
/// Directory holding Spectron's `tests/Results/<name>.png` references
/// (e.g. `…/emulators/zx-spectrum/Spectron/tests/Results`). When set,
/// the smokes additionally byte-compare their 256×192 screen content
/// against Spectron's — a trusted timing-accurate oracle, not just our
/// own locked golden. Unset → that extra check is skipped.
const SPECTRON_RESULTS_ENV: &str = "EMU198X_SPECTRON_RESULTS_DIR";

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
    chord(
        machine,
        &mut frame,
        SpectrumKey::SymbolShift,
        SpectrumKey::P,
    );
    chord(
        machine,
        &mut frame,
        SpectrumKey::SymbolShift,
        SpectrumKey::P,
    );
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

/// Map an 8-bit RGB triple to a ZX Spectrum colour index (0–15):
/// `bright<<3 | green<<2 | red<<1 | blue`. Normal colours use the 215
/// component value, bright ones 255, with each channel either off (0)
/// or on — so a single `== 255` check distinguishes bright, and `> 0`
/// gives each colour bit. Both Spectron's RGB output and our palette
/// resolve to the same indices, making the comparison palette-RGB
/// independent.
fn rgb_to_spectrum_index(r: u8, g: u8, b: u8) -> u8 {
    let bright = if r == 255 || g == 255 || b == 255 {
        8
    } else {
        0
    };
    bright | (if g > 0 { 4 } else { 0 }) | (if r > 0 { 2 } else { 0 }) | (if b > 0 { 1 } else { 0 })
}

/// Resolve one of our framebuffer's palette indices to a Spectrum
/// colour index via `SPECTRUM_PALETTE`.
fn our_pixel_to_spectrum_index(palette_index: u8) -> u8 {
    let entry = SPECTRUM_PALETTE[palette_index as usize & 0x0F];
    rgb_to_spectrum_index(
        ((entry >> 24) & 0xFF) as u8,
        ((entry >> 16) & 0xFF) as u8,
        ((entry >> 8) & 0xFF) as u8,
    )
}

/// Decode a Spectron reference PNG, verify it is a clean 4× nearest-
/// neighbour scale of the raw ULA framebuffer, downscale it back, and
/// map every pixel to a Spectrum colour index. Returns `(indices,
/// raw_width, raw_height)`. Spectron renders the same 256-px-wide
/// content with a symmetric horizontal border, so `raw_width` carries
/// the per-render border size.
fn load_spectron_indices(path: &Path) -> (Vec<u8>, usize, usize) {
    let file = std::fs::File::open(path).expect("open spectron reference");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("decode spectron header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("decode spectron frame");
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("spectron ref {} has colour type {other:?}", path.display()),
    };
    assert!(
        w % 4 == 0 && h % 4 == 0,
        "spectron ref {} is {w}×{h}, not a 4× scale",
        path.display()
    );
    let (rw, rh) = (w / 4, h / 4);
    // Verify the 4× nearest-neighbour scale: each 4×4 block is uniform.
    for by in 0..rh {
        for bx in 0..rw {
            let base = ((by * 4) * w + (bx * 4)) * channels;
            let c = &buf[base..base + 3];
            for dy in 0..4 {
                for dx in 0..4 {
                    let o = ((by * 4 + dy) * w + (bx * 4 + dx)) * channels;
                    assert_eq!(
                        &buf[o..o + 3],
                        c,
                        "spectron ref {} is not a clean 4× scale at block ({bx},{by})",
                        path.display()
                    );
                }
            }
        }
    }
    let mut out = vec![0u8; rw * rh];
    for y in 0..rh {
        for x in 0..rw {
            let o = ((y * 4) * w + (x * 4)) * channels;
            out[y * rw + x] = rgb_to_spectrum_index(buf[o], buf[o + 1], buf[o + 2]);
        }
    }
    (out, rw, rh)
}

/// Byte-compare our 256×192 screen content against Spectron's reference,
/// both reduced to Spectrum colour indices. Our screen content sits at
/// (48, 52) in the 352×296 framebuffer; Spectron's vertical screen
/// origin varies with its render border, so it's found as the alignment
/// that maximises the match and the assertion is that the best alignment
/// is *exact*. A non-exact best alignment means a real rendering/timing
/// difference from the reference. No-op when the references aren't
/// installed.
fn assert_screen_matches_spectron(spectron_png: &str, framebuffer: &[u8]) {
    let Some(dir) = std::env::var_os(SPECTRON_RESULTS_ENV) else {
        return;
    };
    let path = PathBuf::from(dir).join(spectron_png);
    if !path.is_file() {
        eprintln!("spectron ref {} not found — skipping", path.display());
        return;
    }
    let (spec, sw, sh) = load_spectron_indices(&path);
    let sbl = (sw - 256) / 2; // symmetric horizontal border
    const OX: usize = 48;
    const OY: usize = 52;
    let our = |x: usize, y: usize| {
        our_pixel_to_spectrum_index(framebuffer[(OY + y) * SCREEN_WIDTH + (OX + x)])
    };
    let (mut best_matches, mut best_sy) = (0usize, 0usize);
    for sy in 0..=(sh - 192) {
        let mut m = 0;
        for y in 0..192 {
            for x in 0..256 {
                if spec[(sy + y) * sw + (sbl + x)] == our(x, y) {
                    m += 1;
                }
            }
        }
        if m > best_matches {
            best_matches = m;
            best_sy = sy;
        }
    }
    let total = 256 * 192;
    assert_eq!(
        best_matches, total,
        "{spectron_png}: 256×192 screen content differs from Spectron — \
         {best_matches}/{total} match at best vertical alignment (spec_y={best_sy})"
    );
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
    run_and_compare_with_spectron(test_name, None);
}

/// As `run_and_compare`, but when `spectron_png` is `Some`, also assert
/// the captured screen content is byte-equal to Spectron's reference of
/// that name (gated on `EMU198X_SPECTRON_RESULTS_DIR`).
fn run_and_compare_with_spectron(test_name: &str, spectron_png: Option<&str>) {
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
    if let Some(reference) = spectron_png {
        assert_screen_matches_spectron(reference, machine.framebuffer());
    }
}

#[test]
#[ignore = "requires local 48K ROM and floatspy.tap; ~100 s wall time at cycle-accurate tape speed"]
fn floatspy_runs_to_completion() {
    // Captures floatspy's interactive menu. After the +3 floating-bus
    // phase fix (#62) the menu's IN() BYTE reads 0, matching Spectron's
    // floatspy_48.png. Compared to the self-locked menu golden here; the
    // self-test-to-completion compare lives in `floatspy_selftest_ok`.
    run_and_compare("floatspy");
}

/// Drive floatspy's self-test (`T`) to completion and byte-compare the
/// finished "Floating bus OK" screen to Spectron's `floatspy_48.png`. This
/// is the end-to-end proof of the floating-bus read-phase fix (#62): the
/// IN() BYTE reads 0 and floatspy reports OK, byte-equal to the oracle.
/// Gated on `EMU198X_SPECTRON_RESULTS_DIR`; ~370 s at cycle-accurate speed.
#[test]
#[ignore = "requires local 48K ROM, floatspy.tap, and EMU198X_SPECTRON_RESULTS_DIR; ~370 s"]
fn floatspy_selftest_ok() {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        return;
    }
    let tap_path = system_tests_dir().join("floatspy.tap");
    if !tap_path.is_file() {
        return;
    }
    let rom = std::fs::read(&rom_path).expect("read 48K ROM");
    let tape_blocks = tap_blocks_to_tape_blocks(
        parse_tap(&std::fs::read(&tap_path).expect("read floatspy.tap"))
            .expect("parse floatspy.tap"),
    );
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&rom).expect("load 48K ROM bytes");
    machine.reset();
    machine.load_tape_blocks(tape_blocks);
    for _ in 0..BOOT_FRAMES {
        machine.run_frame();
    }
    let after = type_load_command(&mut machine, BOOT_FRAMES);
    for _ in 0..30 {
        machine.run_frame();
    }
    machine.play_tape();
    for _ in (after + 30)..(after + 30 + RUN_BUDGET_FRAMES) {
        machine.run_frame();
    }
    machine.keyboard_mut().set_key(SpectrumKey::T, true);
    for _ in 0..4 {
        machine.run_frame();
    }
    machine.keyboard_mut().set_key(SpectrumKey::T, false);
    for _ in 0..40_000 {
        machine.run_frame();
    }
    let dump = std::env::temp_dir().join("floatspy-selftest.png");
    write_indexed_png(&dump, machine.framebuffer());
    eprintln!("floatspy self-test screen written to {}", dump.display());
    assert_screen_matches_spectron("floatspy_48.png", machine.framebuffer());
}

#[test]
#[ignore = "requires local 48K ROM and halt2int.tap; ~100 s wall time"]
fn halt2int_runs_to_completion() {
    // Spectron parity pending — see `floatspy_runs_to_completion` (#10).
    run_and_compare("halt2int");
}

#[test]
#[ignore = "requires local 48K ROM and btime.tap; ~100 s wall time"]
fn btime_runs_to_completion() {
    run_and_compare_with_spectron("btime", Some("btime_48.png"));
}

#[test]
#[ignore = "requires local 48K ROM and ptime.tap; ~100 s wall time"]
fn ptime_runs_to_completion() {
    // Spectron ships only a 128K ptime reference (`ptime_128.png`); there
    // is no 48K one to validly compare this 48K run against, so it stays
    // on its self-locked golden. (#10)
    run_and_compare("ptime");
}

// Super HALT Invaders Test is 128K-only; see the 128K crate's
// `tape_smoke.rs` for its test wiring.
