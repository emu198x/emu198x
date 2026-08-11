//! Golden-screenshot tests for 128K-specific ULA / timing TAPs.
//!
//! Same shape as the 48K equivalent
//! (`machine-sinclair-zx-spectrum-48k/tests/tape_smoke.rs`):
//! boot, press ENTER on the 128K firmware menu to select the Tape
//! Loader, play the TAP, run the budget, encode the 352×296
//! paletted framebuffer as a 16-colour indexed PNG, and compare to
//! the checked-in golden in `tests/goldens/`. Re-run with
//! `UPDATE_GOLDENS=1` to refresh after eyeballing a deliberate
//! change. HALT2INT is a semantic exception: it decodes the finished
//! screen and requires the diagnostic's complete `HALT: Early`
//! classification instead of preserving an obsolete self-golden.

use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::tape::TapeBlock;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use format_sinclair_zx_spectrum_tap::{TapBlock, parse_tap};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
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

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

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

fn compare_or_update(png_stem: &str, framebuffer: &[u8]) {
    let path = goldens_dir().join(format!("{png_stem}.png"));
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
    let live_path = std::env::temp_dir().join(format!("{png_stem}-live.png"));
    write_indexed_png(&live_path, framebuffer);
    panic!(
        "framebuffer for {png_stem} differs from {} ({differing} of {} pixels). \
         Live frame written to {} for visual diff. \
         Re-run with UPDATE_GOLDENS=1 to refresh after eyeballing the change.",
        path.display(),
        framebuffer.len(),
        live_path.display(),
    );
}

fn run_to_completion(tap_filename: &str) -> Option<Spectrum128K> {
    let rom0_path = rom0_path();
    let rom1_path = rom1_path();
    if !rom0_path.is_file() || !rom1_path.is_file() {
        eprintln!("128K ROMs not found — skipping");
        return None;
    }
    let tap_path = system_tests_dir().join(tap_filename);
    if !tap_path.is_file() {
        eprintln!(
            "{tap_filename} not found at {} — skipping",
            tap_path.display()
        );
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

    Some(machine)
}

fn run_and_compare(tap_filename: &str, png_stem: &str) {
    let Some(machine) = run_to_completion(tap_filename) else {
        return;
    };

    compare_or_update(png_stem, &machine.framebuffer);
}

/// Decode the 24×32 ROM-font text cells in the fixed screen bank.
///
/// HALT2INT prints through ROM 1 (48 BASIC). Reading that ROM bank
/// directly keeps glyph decoding independent of the final paging state.
fn screen_text_lines(machine: &Spectrum128K) -> Vec<String> {
    common_sinclair_zx_spectrum::screen_text::decode_screen_text(
        // Glyphs from ROM 1 (48 BASIC) explicitly, so decoding does
        // not depend on whichever bank is paged in at capture time.
        |addr| machine.memory.read_rom_byte(1, addr),
        |addr| machine.memory.read(addr),
    )
}

#[test]
#[ignore = "requires local 128K ROMs and halt2int128.tap; ~100 s wall time"]
fn halt2int128_runs_to_completion() {
    let Some(machine) = run_to_completion("halt2int128.tap") else {
        emu198x_test_skip::skip!("Spectrum 128K ROMs or tape image not staged");
    };
    let lines = screen_text_lines(&machine);

    assert!(
        lines.iter().any(|line| line.contains("HALT: Early")),
        "HALT2INT128 should classify the complete HALT profile as Early; decoded screen:\n{}",
        lines.join("\n"),
    );
}

#[test]
#[ignore = "requires local 128K ROMs and Super HALT Invaders TAP; ~120 s wall time"]
fn super_halt_invaders_runs_to_completion() {
    run_and_compare(
        "Super HALT Invaders Test (2021-10-07)(Woodmass, Mark)[!].tap",
        "super-halt-invaders",
    );
}
