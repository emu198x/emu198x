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
//! `goldens.rs` for the boot screens. HALT2INT decodes the finished
//! screen and asserts the `HALT: Early` result on top of the pixel
//! compare. Its doc used to call the floating-bus classification "not
//! yet an authority" and check only the HALT half — which was right to
//! be suspicious and wrong to stop there: with Spectron's reference
//! wired in, that classification reads `Unknown` where Spectron gets
//! `Early`, every other value on the screen being identical (#940).
//!
//! Each smoke also byte-compares its 256×192 screen content against
//! Spectron's `tests/Results/<name>_48.png` reference — a trusted
//! timing-accurate oracle, not just our own golden (#10). Those
//! references are checked in at `test-data/spectrum/spectron-results/`,
//! so this runs with no environment set up;
//! `EMU198X_SPECTRON_RESULTS_DIR` overrides the location for the
//! nightly, which provisions its own copy. Spectron's PNGs turn out to
//! be clean 4× nearest-neighbour scales of its raw framebuffer, so
//! after downscaling and mapping both sides to Spectrum colour indices
//! the content compares exactly. `btime` matches byte-for-byte today;
//! `floatspy` needs run-to-completion input driving before its menu
//! capture can be compared meaningfully; `ptime` has no 48K reference
//! to compare against. The self-locked goldens stay as a second
//! regression contract for those tests: drift in tape timing, BASIC
//! interpreter cycle budget, ULA contention, or the Z80's bus probe all
//! show up as a pixel diff.
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
use std::path::{Path, PathBuf};

const ROM_PATH_ENV: &str = "EMU198X_SPECTRUM_48K_ROM";
const SYSTEM_TESTS_DIR_ENV: &str = "EMU198X_SPECTRUM_SYSTEM_TESTS_DIR";
/// Overrides the directory holding Spectron's `tests/Results/<name>.png`
/// references, so the nightly can point at its own provisioned bundle.
/// Unset — the developer default — the references checked in under
/// [`SPECTRON_RESULTS_CHECKED_IN`] are used.
const SPECTRON_RESULTS_ENV: &str = "EMU198X_SPECTRON_RESULTS_DIR";

/// The checked-in Spectron references, relative to this crate.
///
/// They live in the repository rather than the private corpus mirror
/// because they are MIT-licensed and small (116 KB), and because a
/// comparator nobody can run is not a comparator: while these were
/// absent from every developer machine, `assert_screen_matches_spectron`
/// skipped, `emu198x-test-skip` reported that as `ok`, and the 48K
/// floating-bus regression of 2026-08-11 went unreproduced locally for
/// five days — the nightly, which has the references, failed nightly
/// throughout (#10, #939). See `test-data/spectrum/spectron-results/`.
const SPECTRON_RESULTS_CHECKED_IN: &str = "../../test-data/spectrum/spectron-results";

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

/// Where Spectron's reference screens are read from: the override when
/// set, otherwise the copies checked in beside this crate.
fn spectron_results_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(SPECTRON_RESULTS_ENV) {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SPECTRON_RESULTS_CHECKED_IN)
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
    assert_screen_scores_against_spectron(spectron_png, framebuffer, 256 * 192);
}

/// The comparator above, but scored against a *recorded* match count rather
/// than a perfect one.
///
/// For screens with a divergence that is already understood and filed. The
/// assertion is still exact — the count must be the recorded number, not
/// merely at least it — so the gate stays live: it fails if the divergence
/// grows, and equally if it shrinks, which is how a fix gets noticed rather
/// than silently absorbed. That is the same ratchet discipline the
/// contention differentials use, and for the same reason: a comparator that
/// tolerates a range stops reporting the thing it was built to report.
///
/// Prefer fixing the divergence. Reach for this only when the alternative is
/// deleting the assertion, which is how `halt2int_48.png` sat unused while
/// its test printed `ok` (#10).
fn assert_screen_scores_against_spectron(
    spectron_png: &str,
    framebuffer: &[u8],
    expected_matches: usize,
) {
    let path = spectron_results_dir().join(spectron_png);
    // Not a skip. These references are checked in, so a missing one is a
    // broken checkout or a wrong name — both worth failing over. Skipping
    // is what made this comparator do nothing for months while reporting
    // `ok`; the only thing it should tolerate is being pointed elsewhere.
    assert!(
        path.is_file(),
        "Spectron reference {} is missing. Checked-in references live in \
         test-data/spectrum/spectron-results/; {SPECTRON_RESULTS_ENV} overrides that.",
        path.display()
    );
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
    // Dump the live screen before asserting. Without this the failure
    // says only how many pixels differ, and the nearest frame to hand is
    // the *initial* screen from `floatspy_runs_to_completion` — a
    // different screen entirely, which makes it very easy to diff the
    // wrong pair and draw a confident wrong conclusion.
    let live_path = std::env::temp_dir().join(format!("{spectron_png}-live.png"));
    write_indexed_png(&live_path, framebuffer);
    eprintln!("Live self-test frame: {}", live_path.display());

    let total = 256 * 192;
    let verdict = if expected_matches == total {
        "differs from Spectron"
    } else {
        "no longer differs from Spectron by the recorded amount"
    };
    assert_eq!(
        best_matches, expected_matches,
        "{spectron_png}: 256×192 screen content {verdict} — \
         {best_matches}/{total} match at best vertical alignment (spec_y={best_sy}), \
         expected {expected_matches}/{total}"
    );
}

/// Decodes the ULA bitmap into its 24×32 ROM-font text cells.
///
/// HALT2INT prints its classifications through the Spectrum ROM, so matching
/// screen cells against the loaded ROM's glyph table gives the test a stable
/// semantic assertion without locking unrelated diagnostic fields.
fn screen_text_lines(machine: &Spectrum48k) -> Vec<String> {
    common_sinclair_zx_spectrum::screen_text::decode_screen_text(
        |addr| machine.read(addr),
        |addr| machine.read(addr),
    )
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
    let Some(machine) = run_to_completion(test_name) else {
        emu198x_test_skip::skip!("Spectrum 48K ROM or {test_name}.tap not staged");
    };

    compare_or_update(test_name, machine.framebuffer());
    if let Some(reference) = spectron_png {
        assert_screen_matches_spectron(reference, machine.framebuffer());
    }
}

/// Loads and runs one system-test TAP to its fixed capture budget.
fn run_to_completion(test_name: &str) -> Option<Spectrum48k> {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        emu198x_test_skip::record(&format!(
            "48K ROM not found at {} — skipping",
            rom_path.display()
        ));
        return None;
    }
    let tap_path = system_tests_dir().join(format!("{test_name}.tap"));
    if !tap_path.is_file() {
        emu198x_test_skip::record(&format!(
            "{}.tap not found at {} — skipping",
            test_name,
            tap_path.display()
        ));
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

    Some(machine)
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and floatspy.tap; ~100 s wall time at cycle-accurate tape speed"]
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
#[ignore = "FIXTURE: requires local 48K ROM, floatspy.tap, and EMU198X_SPECTRON_RESULTS_DIR; ~370 s"]
fn floatspy_selftest_ok() {
    let rom_path = rom_path();
    if !rom_path.is_file() {
        emu198x_test_skip::skip!("Spectrum 48K ROM not staged (EMU198X_SPECTRUM_48K_ROM)");
    }
    let tap_path = system_tests_dir().join("floatspy.tap");
    if !tap_path.is_file() {
        emu198x_test_skip::skip!("floatspy.tap not staged (EMU198X_SPECTRUM_SYSTEM_TESTS_DIR)");
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
#[ignore = "FIXTURE: requires local 48K ROM and halt2int.tap; ~100 s wall time"]
fn halt2int_runs_to_completion() {
    let Some(machine) = run_to_completion("halt2int") else {
        emu198x_test_skip::skip!(
            "Spectrum 48K ROM or tape image not staged (EMU198X_SPECTRUM_48K_ROM)"
        );
    };
    let lines = screen_text_lines(&machine);

    assert!(
        lines.iter().any(|line| line.contains("HALT: Early")),
        "HALT2INT should classify the HALT profile as Early; decoded screen:\n{}",
        lines.join("\n"),
    );

    // HALT2INT's other classification, which is the one that was wrong.
    // It decides this by stamping `$5800` and reading the floating bus at
    // a fixed instant: at the old read origin the read missed the
    // attribute slot and the suite printed `Float: Unknown` (#940).
    assert!(
        lines.iter().any(|line| line.contains("Float: Early")),
        "HALT2INT should classify the floating bus as Early; decoded screen:\n{}",
        lines.join("\n"),
    );

    // And hold the whole 256x192 to the oracle, because the decoded-text
    // checks above pass on a screen that is wrong everywhere the text is
    // right. `halt2int_48.png` sat unused in Spectron's results until #10.
    //
    // This was a scored ratchet at 49104 of 49152 while #940 stood — the
    // 48 pixels of that one word. It is an exact match now, so it is
    // asserted as one.
    assert_screen_matches_spectron("halt2int_48.png", machine.framebuffer());
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and btime.tap; ~100 s wall time"]
fn btime_runs_to_completion() {
    run_and_compare_with_spectron("btime", Some("btime_48.png"));
}

#[test]
#[ignore = "FIXTURE: requires local 48K ROM and ptime.tap; ~100 s wall time"]
fn ptime_runs_to_completion() {
    // Spectron ships only a 128K ptime reference (`ptime_128.png`); there
    // is no 48K one to validly compare this 48K run against, so it stays
    // on its self-locked golden. (#10)
    run_and_compare("ptime");
}

// Super HALT Invaders Test is 128K-only; see the 128K crate's
// `tape_smoke.rs` for its test wiring.
