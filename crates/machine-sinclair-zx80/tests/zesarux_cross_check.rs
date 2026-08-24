//! Cross-check the ZX80 boot screen against ZEsarUX, per scan line.
//!
//! A second independent implementation beside the MAME check next door, which
//! is what #295 asks for: that issue names EightyOne or zxsp, and ZEsarUX is
//! here instead because it is the one this repo can drive headlessly and
//! reproducibly — `tools/zx8x-zesarux-capture/` builds and captures it. The
//! claim is the same either way: two implementations agree on what this ROM
//! draws and where in the field it draws it.
//!
//! # Per scan line
//!
//! Divergence is reported per scan line rather than as one total, because the
//! ULA composes the picture a line at a time: a fault in the line counter, the
//! sync, or the pattern address shows up as a *band* of wrong lines, and a
//! single number cannot tell that from noise spread over the frame. The report
//! names the first line that differs and how many of its 256 pixels do, which
//! is what the display rewrite this scaffolds will need.
//!
//! # Alignment
//!
//! ZEsarUX renders both Sinclair machines into one 352x296 raster. The ZX80's
//! text area starts at (48, 40) in it, against the ZX81's (50, 41) — the two
//! pictures sit within a couple of pixels of each other, where MAME separates
//! them by 26. Neither figure is asserted as agreement here; that
//! disagreement is #1123, and this test compares the picture rather than its
//! placement.
//!
//! Both origins were found by matching ink point-sets rather than assumed, and
//! the boot screen carries about fifty ink pixels — all cursor — so the match
//! is exact rather than a best fit.
//!
//! # Producing a capture
//!
//! ```text
//! tools/zx8x-zesarux-capture/build.sh /tmp/zesarux
//! tools/zx8x-zesarux-capture/capture.sh /tmp/zesarux/zesarux /tmp/zx80.bmp --machine ZX80
//! EMU198X_ZX80_ZESARUX_BMP=/tmp/zx80.bmp cargo test -p machine-sinclair-zx80 --test zesarux_cross_check
//! ```
//!
//! Skips when no capture is supplied.

use std::{env, fs, path::PathBuf};

/// Where our text area begins.
const OUR_X: u32 = 32;
/// And ZEsarUX's, in its own raster.
const ZESARUX_X: u32 = 48;
const ZESARUX_Y: u32 = 40;
const TEXT_W: u32 = 256;
const TEXT_H: u32 = 192;

/// A 24-bit bottom-up BMP, which is what ZEsarUX's `save-screen` writes.
///
/// Hand-rolled because the alternative is a dependency for one test, and
/// because ZEsarUX writes exactly one shape of file.
struct Bmp {
    data: Vec<u8>,
    offset: usize,
    width: u32,
    height: u32,
    row: usize,
    bytes_per_pixel: usize,
    bottom_up: bool,
}

impl Bmp {
    fn read(path: &PathBuf) -> Option<Self> {
        let data = fs::read(path).ok()?;
        if data.len() < 54 || &data[..2] != b"BM" {
            return None;
        }
        let u32_at = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(o..o + 4)?.try_into().ok()?))
        };
        let offset = u32_at(10)? as usize;
        let width = u32_at(18)?;
        let signed_height = i32::from_le_bytes(data[22..26].try_into().ok()?);
        let bpp = u16::from_le_bytes(data[28..30].try_into().ok()?) as usize;
        if !bpp.is_multiple_of(8) || width == 0 {
            return None;
        }
        let bytes_per_pixel = bpp / 8;
        Some(Self {
            offset,
            width,
            height: signed_height.unsigned_abs(),
            row: (width as usize * bpp).div_ceil(32) * 4,
            bytes_per_pixel,
            bottom_up: signed_height > 0,
            data,
        })
    }

    fn ink(&self, x: u32, y: u32) -> bool {
        let row = if self.bottom_up {
            self.height - 1 - y
        } else {
            y
        };
        let at = self.offset + row as usize * self.row + x as usize * self.bytes_per_pixel;
        self.data.get(at).is_some_and(|&v| v < 128)
    }
}

/// Divergence per scan line: `(row, pixels differing)`, rows that agree
/// omitted.
fn scanline_diff(
    ours: &dyn Fn(u32, u32) -> bool,
    theirs: &Bmp,
    origin: (u32, u32),
    lines: std::ops::Range<u32>,
) -> Vec<(u32, u32)> {
    let (zx, zy) = origin;
    lines
        .filter_map(|row| {
            let n = (0..TEXT_W)
                .filter(|col| ours(OUR_X + col, row) != theirs.ink(zx + col, zy + row))
                .count();
            #[allow(clippy::cast_possible_truncation)]
            (n > 0).then_some((row, n as u32))
        })
        .collect()
}

/// A report an operator can act on: how many lines, which came first, and how
/// badly.
fn report(diffs: &[(u32, u32)]) -> String {
    let total: u32 = diffs.iter().map(|&(_, n)| n).sum();
    let worst = diffs.iter().max_by_key(|&&(_, n)| n).copied();
    format!(
        "{} scan lines differ ({total} pixels). First: line {} with {} of {TEXT_W}. \
         Worst: line {} with {} of {TEXT_W}.",
        diffs.len(),
        diffs[0].0,
        diffs[0].1,
        worst.map_or(0, |(r, _)| r),
        worst.map_or(0, |(_, n)| n),
    )
}

/// Our committed boot golden, as a pixel lookup in framebuffer coordinates.
fn golden(name: &str) -> impl Fn(u32, u32) -> bool {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    let decoder = png::Decoder::new(std::io::BufReader::new(
        fs::File::open(&path).expect("golden"),
    ));
    let mut reader = decoder.read_info().expect("golden header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("golden size")];
    let info = reader.next_frame(&mut buf).expect("golden pixels");
    let bpp = info.buffer_size() / (info.width as usize * info.height as usize);
    let width = info.width as usize;
    move |x: u32, y: u32| buf[(y as usize * width + x as usize) * bpp] < 128
}

fn capture(var: &str) -> Option<Bmp> {
    let path = PathBuf::from(env::var(var).ok()?);
    Some(Bmp::read(&path).unwrap_or_else(|| panic!("could not read a BMP at {}", path.display())))
}

#[test]
fn the_text_area_matches_zesarux() {
    let Some(theirs) = capture("EMU198X_ZX80_ZESARUX_BMP") else {
        emu198x_test_skip::skip!(
            "no ZEsarUX capture — see tools/zx8x-zesarux-capture/ and set EMU198X_ZX80_ZESARUX_BMP"
        );
    };
    assert!(
        theirs.width >= ZESARUX_X + TEXT_W && theirs.height >= ZESARUX_Y + TEXT_H,
        "the capture is {}x{}, too small to hold the text area at ({ZESARUX_X}, {ZESARUX_Y}) — \
         ZEsarUX's raster has moved and the offsets here need re-deriving",
        theirs.width,
        theirs.height
    );

    let ours = golden("zx80-boot.png");
    let our_y = machine_sinclair_zx80::TelevisionStandard::FiftyHz.text_top();
    let shifted = |x: u32, y: u32| ours(x, our_y + y);
    let diffs = scanline_diff(&shifted, &theirs, (ZESARUX_X, ZESARUX_Y), 0..TEXT_H);

    assert!(
        diffs.is_empty(),
        "the boot screen should be pixel-identical to ZEsarUX's. {}",
        report(&diffs)
    );
}
