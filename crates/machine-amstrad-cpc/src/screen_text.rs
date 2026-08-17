//! Reading the CPC's bitmap screen back as text.
//!
//! The CPC has no character memory. Text is drawn as pixels, so anything that
//! wants to *read* the screen — a test asserting on a firmware message, a
//! harness scoring a test suite's report — has to undo the drawing. Before
//! this existed, `machine-amstrad-cpc`'s tests could assert only that some
//! number of screen bytes were non-zero, which proves something was drawn and
//! never what.
//!
//! Three things have to line up.
//!
//! **The layout is not row-major.** The CRTC fetches 80 bytes per scanline,
//! but the eight scanlines of a character row are 2 KB apart, not 80 bytes.
//! A character row's line `l` lives at `base + l * 0x800 + row * 80`. The
//! screen is 16 KB because the eighth block ends exactly at the top of it.
//!
//! **Pixels are interleaved inside a byte.** Mode 1 packs four 2-bit pens
//! into a byte, but not as four adjacent pairs: pen `n` takes its high bit
//! from bit `7-n` and its low bit from bit `3-n`. Mode 0 spreads four bits
//! across the same byte for two pixels. Only mode 2 is the obvious thing —
//! one bit per pixel, bit 7 leftmost.
//!
//! **The font is in ROM.** The firmware's 8×8 matrices for all 256 characters
//! sit in the last 2 KB of the 16 KB OS ROM, at `&3800`, bit 7 leftmost. That
//! is what [`FONT_OFFSET`] points at.
//!
//! Matching is exact, against the glyph and its inverse, because the firmware
//! draws inverse video by swapping pen and paper rather than by holding a
//! second set of matrices. A cell matching neither is reported as `?` unless
//! it is blank, which keeps a decode honest instead of quietly returning
//! plausible-looking text — software with its own font will come back as a
//! wall of `?`, and that is the correct answer rather than a silent one.

use amstrad_gate_array::VideoMode;

/// Offset of the firmware's character matrices within the 16 KB OS ROM.
pub const FONT_OFFSET: usize = 0x3800;
/// One glyph: eight rows, one byte each, bit 7 leftmost.
pub const GLYPH_BYTES: usize = 8;
/// Bytes the CRTC fetches per scanline, in every mode.
const BYTES_PER_LINE: u16 = 80;
/// Distance between successive scanlines of the same character row.
const LINE_BLOCK: u16 = 0x800;
/// Character rows on a standard 200-line screen.
const TEXT_ROWS: usize = 25;

/// Character columns, and bytes per character cell, for a mode.
///
/// Every mode is eight pixels to the character; they differ only in how many
/// bytes those eight pixels occupy. Both fall out of the Gate Array's own
/// [`VideoMode::pixels_per_byte`], so this cannot drift from the renderer.
fn geometry(mode: VideoMode) -> (usize, u16) {
    let cell_bytes = 8 / mode.pixels_per_byte();
    (
        usize::from(BYTES_PER_LINE) / cell_bytes,
        u16::try_from(cell_bytes).expect("1, 2 or 4"),
    )
}

/// Decode the screen into 25 rows of text.
///
/// `read_byte` must reach **RAM**, not the CPU's read map: the upper ROM is
/// usually paged in over `&C000`, so a read that honours paging returns BASIC
/// rather than the screen.
///
/// `os_rom` is the 16 KB lower ROM, which carries the font. `base` is the
/// CRTC's display start address, already converted to a byte address.
///
/// Pen 0 is treated as paper and everything else as ink, which is what the
/// firmware does and what any software using the standard text routines
/// inherits.
pub fn decode_screen_text(
    read_byte: impl Fn(u16) -> u8,
    os_rom: &[u8],
    mode: VideoMode,
    base: u16,
) -> Vec<String> {
    let font = &os_rom[FONT_OFFSET..FONT_OFFSET + 256 * GLYPH_BYTES];
    let (columns, cell_bytes) = geometry(mode);
    let ppb = mode.pixels_per_byte();

    let mut rows = Vec::with_capacity(TEXT_ROWS);
    for row in 0..TEXT_ROWS {
        let mut line = String::with_capacity(columns);
        for col in 0..columns {
            let mut glyph = [0u8; GLYPH_BYTES];
            for (l, slot) in glyph.iter_mut().enumerate() {
                let addr = base
                    .wrapping_add(u16::try_from(l).expect("0..8") * LINE_BLOCK)
                    .wrapping_add(u16::try_from(row).expect("0..25") * BYTES_PER_LINE)
                    .wrapping_add(u16::try_from(col).expect("column") * cell_bytes);
                let mut mask = 0u8;
                for b in 0..usize::from(cell_bytes) {
                    let byte = read_byte(addr.wrapping_add(u16::try_from(b).expect("cell")));
                    // Successive pixels come from shifting the byte left, which
                    // is the contract `leftmost_pen` documents.
                    for p in 0..ppb {
                        if mode.leftmost_pen(byte << p) != 0 {
                            mask |= 0x80 >> (b * ppb + p);
                        }
                    }
                }
                *slot = mask;
            }
            line.push(match_glyph(&glyph, font));
        }
        rows.push(line.trim_end().to_owned());
    }
    rows
}

/// Match an 8×8 cell to a character, trying the inverse too.
fn match_glyph(glyph: &[u8; GLYPH_BYTES], font: &[u8]) -> char {
    if glyph.iter().all(|&b| b == 0) {
        return ' ';
    }
    let inverse: Vec<u8> = glyph.iter().map(|b| !b).collect();
    for c in 0..256usize {
        let m = &font[c * GLYPH_BYTES..(c + 1) * GLYPH_BYTES];
        if m == glyph.as_slice() || m == inverse.as_slice() {
            // Control codes have matrices too, but a screen never shows one;
            // a match there means an accidental collision, not text.
            let ch = u8::try_from(c).expect("0..256");
            if (0x20..=0x7E).contains(&ch) {
                return char::from(ch);
            }
        }
    }
    '?'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight pixels to the character in every mode, filling the CRTC's
    /// 80-byte line exactly. Getting this wrong shifts every column.
    #[test]
    fn geometry_is_eight_pixels_to_the_character_in_every_mode() {
        for mode in [
            VideoMode::Mode0,
            VideoMode::Mode1,
            VideoMode::Mode2,
            VideoMode::Mode3,
        ] {
            let (cols, bytes) = geometry(mode);
            assert_eq!(
                usize::from(bytes) * mode.pixels_per_byte(),
                8,
                "{mode:?} cell is not eight pixels wide"
            );
            assert_eq!(
                cols * usize::from(bytes),
                usize::from(BYTES_PER_LINE),
                "{mode:?} row does not fill the CRTC's 80 bytes"
            );
        }
    }

    /// A blank cell is a space, and an unrecognised one is `?` rather than a
    /// guess.
    #[test]
    fn unmatched_cells_report_themselves() {
        let font = vec![0u8; 256 * GLYPH_BYTES];
        assert_eq!(match_glyph(&[0; 8], &font), ' ');
        assert_eq!(match_glyph(&[0b1010_1010; 8], &font), '?');
    }
}
