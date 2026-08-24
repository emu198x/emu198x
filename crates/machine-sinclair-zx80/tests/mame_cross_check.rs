//! Cross-check the ZX80 boot screen against MAME.
//!
//! This is what turns the golden next door from a regression baseline into an
//! accuracy reference. `knowledge/processes/golden-image-capture.md` requires
//! provenance recording an **external** capture, and the Amiga boot goldens
//! set the precedent by comparing against FS-UAE. MAME 0.289 plays the same
//! part here.
//!
//! It is another emulator, not hardware, and the claim is exactly that: two
//! independent implementations agree, pixel for pixel, on what this ROM draws
//! and where in the field it draws it. That is worth a great deal and it is
//! not the same as measuring a real machine.
//!
//! # Aligning two different rasters
//!
//! MAME renders the whole 384x311 field, closing with the six blank
//! lines of vertical sync. We render the 320x288 window a set shows,
//! starting `FIRST_VISIBLE_LINE` into the field. So MAME's row *n* is our row
//! *n* - 8, and the comparison is over the 256x192 text area both contain.
//!
//! The vertical offset is the interesting half. It comes out at exactly 8,
//! which is `FIRST_VISIBLE_LINE` — derived in #1116 from the ROM's pad and
//! never checked against anything until now.
//!
//! Horizontally the two disagree, and deliberately not asserted as agreement:
//! MAME puts the ZX80's picture 26 pixels right of the ZX81's, while we place
//! both at the same column. Ours is `FIRST_CHAR_TSTATE`, a constant fitted to
//! a window that had already been chosen — the circularity #1054 records. See
//! #1123.
//!
//! # Producing the capture
//!
//! ```text
//! tools/zx8x-mame-capture/capture.sh /tmp/zx8x
//! EMU198X_ZX80_MAME_PNG=/tmp/zx8x/zx80-boot-mame.png cargo test -p machine-sinclair-zx80 --test mame_cross_check
//! ```
//!
//! Skips when no capture is supplied, so an ordinary run is unaffected.

use std::{env, fs, io::BufReader, path::PathBuf};

/// Framebuffer column and row where our text area begins.
const OUR_X: u32 = 32;
/// MAME's column for the same character, and the frame line it starts on.
const MAME_X: u32 = 80;
const MAME_Y: u32 = 56;
const TEXT_W: u32 = 256;
const TEXT_H: u32 = 192;

fn read_png(path: &PathBuf) -> Option<(Vec<u8>, u32, u32, usize)> {
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(path).ok()?));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let bpp = info.buffer_size() / (info.width as usize * info.height as usize);
    buf.truncate(info.buffer_size());
    Some((buf, info.width, info.height, bpp))
}

#[test]
fn the_text_area_matches_mame() {
    let Ok(mame_path) = env::var("EMU198X_ZX80_MAME_PNG") else {
        emu198x_test_skip::skip!(
            "no MAME capture — run tools/zx8x-mame-capture/capture.sh and set EMU198X_ZX80_MAME_PNG"
        );
    };
    let mame_path = PathBuf::from(mame_path);
    let Some((mame, mw, mh, mbpp)) = read_png(&mame_path) else {
        panic!("could not read the MAME capture at {}", mame_path.display());
    };

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/zx80-boot.png");
    let Some((ours, ow, _oh, obpp)) = read_png(&golden_path) else {
        panic!("could not read the golden at {}", golden_path.display());
    };

    assert!(
        mw >= MAME_X + TEXT_W && mh >= MAME_Y + TEXT_H,
        "the capture is {mw}x{mh}, too small to hold the text area at ({MAME_X}, {MAME_Y}) — \
         MAME's raster has moved and the offsets here need re-deriving"
    );

    let our_y = machine_sinclair_zx80::TelevisionStandard::FiftyHz.text_top();
    let dark = |buf: &[u8], w: u32, bpp: usize, x: u32, y: u32| {
        buf[(y as usize * w as usize + x as usize) * bpp] < 128
    };

    let mut differing = 0u32;
    let mut first = None;
    for row in 0..TEXT_H {
        for col in 0..TEXT_W {
            let a = dark(&ours, ow, obpp, OUR_X + col, our_y + row);
            let b = dark(&mame, mw, mbpp, MAME_X + col, MAME_Y + row);
            if a != b {
                differing += 1;
                if first.is_none() {
                    first = Some((col, row));
                }
            }
        }
    }

    assert_eq!(
        differing,
        0,
        "the text area should be pixel-identical to MAME's; {differing} of {} pixels differ, \
         first at column {:?} of the text area",
        TEXT_W * TEXT_H,
        first
    );

    // Stated separately because it is the claim worth making: our window
    // opens `FIRST_VISIBLE_LINE` into the field, and the text lands where
    // MAME says it should once that offset is applied.
    assert_eq!(
        MAME_Y - our_y,
        8,
        "the field-relative placement should differ by exactly FIRST_VISIBLE_LINE"
    );
}
