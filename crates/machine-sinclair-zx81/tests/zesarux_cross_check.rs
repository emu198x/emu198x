//! Cross-check the ZX81 boot screen against ZEsarUX.
//!
//! A second independent implementation beside the MAME check next door, and
//! the one that matters for what comes next: MAME's ZX81 driver implements
//! only the character path, so it can never be the reference for WRX (#301).
//! ZEsarUX implements both.
//!
//! # Alignment
//!
//! ZEsarUX renders a 352x296 raster closing with seven blank lines of vertical
//! sync at rows 289-295 — frame lines 304-310, which is where our own field
//! arithmetic puts the sync. Its row 0 is therefore frame line 15, against our
//! 8, so **our row is its row plus seven**.
//!
//! Horizontally its text area starts at column 50 where ours starts at 32.
//! That offset is not asserted as agreement, for the same reason the MAME
//! check does not assert one: `FIRST_CHAR_TSTATE` is fitted per machine, which
//! is #1123.
//!
//! Both origins were found by exhaustive search rather than assumed, and the
//! match at (50, 41) is exact — zero differing pixels of 49,152 — while every
//! neighbouring origin differs by 24 or more. That sharpness is what makes it
//! an alignment rather than a coincidence.
//!
//! # Producing a capture
//!
//! ```text
//! tools/zx8x-zesarux-capture/build.sh /tmp/zesarux
//! tools/zx8x-zesarux-capture/capture.sh /tmp/zesarux/zesarux /tmp/zx81.bmp
//! EMU198X_ZX81_ZESARUX_BMP=/tmp/zx81.bmp cargo test -p machine-sinclair-zx81 --test zesarux_cross_check
//! ```
//!
//! Skips when no capture is supplied.

use std::{env, fs, path::PathBuf};

/// Where our text area begins.
const OUR_X: u32 = 32;
/// And ZEsarUX's, in its own raster.
const ZESARUX_X: u32 = 50;
const ZESARUX_Y: u32 = 41;
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

#[test]
fn the_text_area_matches_zesarux() {
    let Ok(path) = env::var("EMU198X_ZX81_ZESARUX_BMP") else {
        emu198x_test_skip::skip!(
            "no ZEsarUX capture — see tools/zx8x-zesarux-capture/ and set EMU198X_ZX81_ZESARUX_BMP"
        );
    };
    let path = PathBuf::from(path);
    let Some(theirs) = Bmp::read(&path) else {
        panic!("could not read a BMP at {}", path.display());
    };
    assert!(
        theirs.width >= ZESARUX_X + TEXT_W && theirs.height >= ZESARUX_Y + TEXT_H,
        "the capture is {}x{}, too small to hold the text area at ({ZESARUX_X}, {ZESARUX_Y}) — \
         ZEsarUX's raster has moved and the offsets here need re-deriving",
        theirs.width,
        theirs.height
    );

    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/zx81-boot.png");
    let decoder = png::Decoder::new(std::io::BufReader::new(
        fs::File::open(&golden).expect("golden"),
    ));
    let mut reader = decoder.read_info().expect("golden header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("golden size")];
    let info = reader.next_frame(&mut buf).expect("golden pixels");
    let bpp = info.buffer_size() / (info.width as usize * info.height as usize);
    let ours = |x: u32, y: u32| buf[(y as usize * info.width as usize + x as usize) * bpp] < 128;

    let our_y = machine_sinclair_zx81::TEXT_TOP;
    let mut differing = 0u32;
    for row in 0..TEXT_H {
        for col in 0..TEXT_W {
            if ours(OUR_X + col, our_y + row) != theirs.ink(ZESARUX_X + col, ZESARUX_Y + row) {
                differing += 1;
            }
        }
    }

    assert_eq!(
        differing,
        0,
        "the text area should be pixel-identical to ZEsarUX's; {differing} of {} differ",
        TEXT_W * TEXT_H
    );
}
