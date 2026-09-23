//! Shared 48K/128K screenshot oracle, extracted from the 48K tape smoke tests.
//! Included by integration tests so PNG remains a test-only dependency.

use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH};
use std::path::{Path, PathBuf};

/// Overrides the directory holding Spectron's `tests/Results/<name>.png`
/// references, so the nightly can point at its own provisioned bundle.
/// Unset — the developer default — the references checked in under
/// [`SPECTRON_RESULTS_CHECKED_IN`] are used.
const SPECTRON_RESULTS_ENV: &str = "EMU198X_SPECTRON_RESULTS_DIR";

/// The checked-in Spectron references, relative to each consuming machine crate.
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

/// Where Spectron's reference screens are read from: the override when
/// set, otherwise the copies checked in beside this crate.
fn spectron_results_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(SPECTRON_RESULTS_ENV) {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SPECTRON_RESULTS_CHECKED_IN)
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

/// Write a full palette-indexed framebuffer for golden checks or failure inspection.
pub fn write_indexed_png(path: &Path, framebuffer: &[u8]) {
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
/// difference from the reference. Missing references fail the test.
pub fn assert_screen_matches_spectron(spectron_png: &str, framebuffer: &[u8]) {
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
    assert!(
        sw >= 256 && sh >= 192,
        "Spectron reference is smaller than the active screen"
    );
    assert_eq!(framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
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
