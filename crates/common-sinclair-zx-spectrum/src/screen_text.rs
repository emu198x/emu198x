//! Decode the display file back into text by matching ROM font glyphs.
//!
//! Spectrum test programs report their results by printing. A pixel
//! comparison can say "this frame changed"; it cannot say *what the
//! machine reported*. Matching each 8×8 character cell against the ROM
//! font recovers the text, which is what turns a screen into an
//! assertion — `HALT2INT128` classifying a profile as `Early`, or the
//! ZXSpectrum4.net timing suite printing `Pass` / `Fail` with its
//! expected values.
//!
//! The two readers are separate because the glyph source and the
//! display file are not always the same address space. A 48K machine
//! reads both through one flat map; a 128K-class machine reads the font
//! from ROM 1 (48 BASIC) explicitly, so that glyph decoding does not
//! depend on whichever bank happens to be paged in when the capture is
//! taken.

/// Character cells across the display file.
pub const SCREEN_TEXT_COLS: usize = 32;
/// Character rows down the display file.
pub const SCREEN_TEXT_ROWS: usize = 24;
/// Address of the ROM font's first glyph.
pub const ROM_TEXT_GLYPH_BASE: u16 = 0x3D00;
/// Character code of the ROM font's first glyph (space).
pub const ROM_TEXT_GLYPH_FIRST: u8 = 0x20;
/// Number of glyphs in the ROM font.
pub const ROM_TEXT_GLYPH_COUNT: usize = 96;

/// Base address of the display file.
const DISPLAY_FILE_BASE: u16 = 0x4000;

/// Display-file address of pixel row `y`, character column `col`.
///
/// The Spectrum's layout is famously non-linear: the third of the
/// screen, the character row within it and the pixel row within the
/// character each occupy a different bit field of the address.
#[must_use]
pub const fn display_address(y: usize, col: usize) -> u16 {
    DISPLAY_FILE_BASE
        + (((y & 0b1100_0000) as u16) << 5)
        + (((y & 0b0011_1000) as u16) << 2)
        + (((y & 0b0000_0111) as u16) << 8)
        + col as u16
}

/// Decode the display file into 24 lines of 32 characters.
///
/// `glyph_at` reads the ROM font; `screen_at` reads the display file.
/// A cell matching no glyph decodes as `?`, so mixed graphics and text
/// degrade to readable text rather than failing.
///
/// Character codes map to ASCII, with `0x7F` decoding as the Spectrum's
/// `©`.
pub fn decode_screen_text(
    glyph_at: impl Fn(u16) -> u8,
    screen_at: impl Fn(u16) -> u8,
) -> Vec<String> {
    let glyphs: Vec<[u8; 8]> = (0..ROM_TEXT_GLYPH_COUNT)
        .map(|glyph_index| {
            let mut glyph = [0u8; 8];
            let glyph_base = ROM_TEXT_GLYPH_BASE + (glyph_index as u16) * 8;
            for (row, byte) in glyph.iter_mut().enumerate() {
                *byte = glyph_at(glyph_base + row as u16);
            }
            glyph
        })
        .collect();

    (0..SCREEN_TEXT_ROWS)
        .map(|text_row| {
            let mut line = String::with_capacity(SCREEN_TEXT_COLS);
            for text_col in 0..SCREEN_TEXT_COLS {
                let mut cell = [0u8; 8];
                for (pixel_row, byte) in cell.iter_mut().enumerate() {
                    *byte = screen_at(display_address(text_row * 8 + pixel_row, text_col));
                }

                line.push(glyphs.iter().position(|glyph| *glyph == cell).map_or(
                    '?',
                    |glyph_index| {
                        let code = ROM_TEXT_GLYPH_FIRST + glyph_index as u8;
                        match code {
                            0x20..=0x7E => code as char,
                            0x7F => '©',
                            _ => '?',
                        }
                    },
                ));
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The display-file layout is the part most easily got wrong, so
    /// pin the three known anchors: the origin, the start of the second
    /// third, and the second pixel row of the top character row.
    #[test]
    fn display_addresses_match_the_known_layout() {
        assert_eq!(display_address(0, 0), 0x4000);
        assert_eq!(display_address(64, 0), 0x4800);
        assert_eq!(display_address(1, 0), 0x4100);
        assert_eq!(display_address(8, 0), 0x4020);
        assert_eq!(display_address(0, 31), 0x401F);
    }

    /// A synthetic font of one glyph per character code round-trips, and
    /// an unrecognised cell degrades to `?` instead of failing.
    #[test]
    fn decodes_glyphs_and_marks_unknown_cells() {
        // Glyph n is eight copies of byte n, so each code is distinct.
        let glyph_at = |addr: u16| -> u8 {
            let index = (addr - ROM_TEXT_GLYPH_BASE) / 8;
            ROM_TEXT_GLYPH_FIRST.wrapping_add(index as u8)
        };
        // Column 0 spells the glyph for 'A'; column 1 is not a glyph.
        let screen_at = |addr: u16| -> u8 {
            let col = addr & 0x1F;
            match col {
                0 => b'A',
                1 => 0xAA,
                _ => b' ',
            }
        };

        let lines = decode_screen_text(glyph_at, screen_at);
        assert_eq!(lines.len(), SCREEN_TEXT_ROWS);
        assert_eq!(lines[0].chars().next(), Some('A'));
        assert_eq!(lines[0].chars().nth(1), Some('?'));
        assert_eq!(lines[0].chars().nth(2), Some(' '));
        assert!(lines.iter().all(|l| l.chars().count() == SCREEN_TEXT_COLS));
    }
}
