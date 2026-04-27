//! Motorola MC6847 Video Display Generator helpers.
//!
//! This first slice models the alphanumeric text page shape used during Dragon
//! ROM bring-up. It deliberately exposes a text snapshot before pixel rendering
//! so we can validate the video RAM contents and SAM display-base wiring.

/// Alphanumeric text columns.
pub const TEXT_COLUMNS: usize = 32;
/// Alphanumeric text rows.
pub const TEXT_ROWS: usize = 16;
/// Text-mode character cell width in pixels.
pub const TEXT_CELL_WIDTH: usize = 8;
/// Text-mode character cell height in pixels.
pub const TEXT_CELL_HEIGHT: usize = 12;
/// Text-mode framebuffer width in pixels.
pub const TEXT_FRAMEBUFFER_WIDTH: usize = TEXT_COLUMNS * TEXT_CELL_WIDTH;
/// Text-mode framebuffer height in pixels.
pub const TEXT_FRAMEBUFFER_HEIGHT: usize = TEXT_ROWS * TEXT_CELL_HEIGHT;
/// Text-mode framebuffer size in pixels.
pub const TEXT_FRAMEBUFFER_PIXELS: usize = TEXT_FRAMEBUFFER_WIDTH * TEXT_FRAMEBUFFER_HEIGHT;
/// Left text-mode border width in pixels.
pub const TEXT_LEFT_BORDER_PIXELS: usize = 60;
/// Right text-mode border width in pixels.
pub const TEXT_RIGHT_BORDER_PIXELS: usize = 56;
/// Top text-mode border height in scanlines.
pub const TEXT_TOP_BORDER_LINES: usize = 25;
/// Bottom text-mode border height in scanlines.
pub const TEXT_BOTTOM_BORDER_LINES: usize = 26;
/// Visible text-mode framebuffer width including borders.
pub const TEXT_VISIBLE_FRAMEBUFFER_WIDTH: usize =
    TEXT_LEFT_BORDER_PIXELS + TEXT_FRAMEBUFFER_WIDTH + TEXT_RIGHT_BORDER_PIXELS;
/// Visible text-mode framebuffer height including borders.
pub const TEXT_VISIBLE_FRAMEBUFFER_HEIGHT: usize =
    TEXT_TOP_BORDER_LINES + TEXT_FRAMEBUFFER_HEIGHT + TEXT_BOTTOM_BORDER_LINES;
/// Visible text-mode framebuffer size in pixels.
pub const TEXT_VISIBLE_FRAMEBUFFER_PIXELS: usize =
    TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;
/// Bytes consumed by one MC6847 text screen.
pub const TEXT_SCREEN_BYTES: usize = TEXT_COLUMNS * TEXT_ROWS;

/// Default Dragon alpha-mode border colour, ARGB8888.
///
/// This is MC6847 `VDG_BLACK` through XRoar's default "ideal" VDG voltage
/// palette and PAL display conversion. It is intentionally not pure black.
pub const DEFAULT_TEXT_BORDER: u32 = 0xFF03_0303;
/// Default Dragon alpha-mode background colour, ARGB8888.
///
/// This is MC6847 `VDG_DARK_GREEN` through the same reference conversion.
pub const DEFAULT_TEXT_BACKGROUND: u32 = 0xFF00_0B00;
/// Default Dragon alpha-mode foreground colour, ARGB8888.
///
/// This is MC6847 `VDG_GREEN` through the same reference conversion.
pub const DEFAULT_TEXT_FOREGROUND: u32 = 0xFF15_8815;

/// Decoded MC6847 text cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextCell {
    /// Original byte read from display memory.
    pub raw: u8,
    /// Displayable ASCII approximation for diagnostics.
    pub ch: char,
    /// Whether the cell has the inverse-video bit set.
    pub inverse: bool,
}

/// A 32x16 alphanumeric text snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextScreen {
    cells: [TextCell; TEXT_SCREEN_BYTES],
}

impl TextScreen {
    /// Capture a 32x16 text screen by reading sequential display bytes.
    #[must_use]
    pub fn capture(mut read_byte: impl FnMut(usize) -> u8) -> Self {
        let cells = std::array::from_fn(|index| decode_text_byte(read_byte(index)));
        Self { cells }
    }

    /// Return the decoded text cell at `row`, `column`.
    #[must_use]
    pub fn cell(&self, row: usize, column: usize) -> Option<TextCell> {
        if row >= TEXT_ROWS || column >= TEXT_COLUMNS {
            return None;
        }
        Some(self.cells[row * TEXT_COLUMNS + column])
    }

    /// Return all cells in row-major order.
    #[must_use]
    pub const fn cells(&self) -> &[TextCell; TEXT_SCREEN_BYTES] {
        &self.cells
    }

    /// Return plain diagnostic text with one line per VDG text row.
    #[must_use]
    pub fn to_plain_text(&self) -> String {
        let mut text = String::with_capacity((TEXT_COLUMNS + 1) * TEXT_ROWS);
        for row in 0..TEXT_ROWS {
            if row != 0 {
                text.push('\n');
            }
            for column in 0..TEXT_COLUMNS {
                text.push(self.cells[row * TEXT_COLUMNS + column].ch);
            }
        }
        text
    }

    /// Render the text screen to an ARGB8888 framebuffer.
    #[must_use]
    pub fn render_argb(&self, palette: TextPalette) -> Vec<u32> {
        let mut framebuffer = vec![palette.background; TEXT_FRAMEBUFFER_PIXELS];
        self.render_argb_into(palette, &mut framebuffer);
        framebuffer
    }

    /// Render the text screen into an existing ARGB8888 framebuffer.
    pub fn render_argb_into(&self, palette: TextPalette, framebuffer: &mut [u32]) {
        assert_eq!(framebuffer.len(), TEXT_FRAMEBUFFER_PIXELS);

        for row in 0..TEXT_ROWS {
            for column in 0..TEXT_COLUMNS {
                let cell = self.cells[row * TEXT_COLUMNS + column];
                render_cell(
                    row,
                    column,
                    cell,
                    palette,
                    RenderTarget {
                        framebuffer,
                        width: TEXT_FRAMEBUFFER_WIDTH,
                        x_origin: 0,
                        y_origin: 0,
                    },
                );
            }
        }
    }

    /// Render the text screen plus visible border to an ARGB8888 framebuffer.
    #[must_use]
    pub fn render_visible_argb(&self, palette: TextPalette) -> Vec<u32> {
        let mut framebuffer = vec![palette.border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS];
        self.render_visible_argb_into(palette, &mut framebuffer);
        framebuffer
    }

    /// Render the text screen plus visible border into an existing ARGB8888 framebuffer.
    pub fn render_visible_argb_into(&self, palette: TextPalette, framebuffer: &mut [u32]) {
        assert_eq!(framebuffer.len(), TEXT_VISIBLE_FRAMEBUFFER_PIXELS);
        framebuffer.fill(palette.border);

        for row in 0..TEXT_ROWS {
            for column in 0..TEXT_COLUMNS {
                let cell = self.cells[row * TEXT_COLUMNS + column];
                render_cell(
                    row,
                    column,
                    cell,
                    palette,
                    RenderTarget {
                        framebuffer,
                        width: TEXT_VISIBLE_FRAMEBUFFER_WIDTH,
                        x_origin: TEXT_LEFT_BORDER_PIXELS,
                        y_origin: TEXT_TOP_BORDER_LINES,
                    },
                );
            }
        }
    }
}

/// Text-mode render palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPalette {
    /// Border colour, ARGB8888.
    pub border: u32,
    /// Background colour, ARGB8888.
    pub background: u32,
    /// Foreground colour, ARGB8888.
    pub foreground: u32,
}

impl Default for TextPalette {
    fn default() -> Self {
        Self {
            border: DEFAULT_TEXT_BORDER,
            background: DEFAULT_TEXT_BACKGROUND,
            foreground: DEFAULT_TEXT_FOREGROUND,
        }
    }
}

/// Decode one MC6847 alphanumeric byte into a diagnostic text cell.
#[must_use]
pub fn decode_text_byte(raw: u8) -> TextCell {
    TextCell {
        raw,
        ch: diagnostic_char(raw),
        inverse: raw & 0x40 != 0,
    }
}

fn diagnostic_char(raw: u8) -> char {
    let code = raw & 0x3F;
    match code {
        0x00 => '@',
        0x01..=0x1A => char::from(b'A' + code - 1),
        0x1B => '[',
        0x1C => '\\',
        0x1D => ']',
        0x1E => '^',
        0x1F => '_',
        0x20 => ' ',
        0x21 => '!',
        0x22 => '"',
        0x23 => '#',
        0x24 => '$',
        0x25 => '%',
        0x26 => '&',
        0x27 => '\'',
        0x28 => '(',
        0x29 => ')',
        0x2A => '*',
        0x2B => '+',
        0x2C => ',',
        0x2D => '-',
        0x2E => '.',
        0x2F => '/',
        0x30..=0x39 => char::from(b'0' + code - 0x30),
        0x3A => ':',
        0x3B => ';',
        0x3C => '<',
        0x3D => '=',
        0x3E => '>',
        0x3F => '?',
        _ => unreachable!("6-bit character code is always in range"),
    }
}

struct RenderTarget<'a> {
    framebuffer: &'a mut [u32],
    width: usize,
    x_origin: usize,
    y_origin: usize,
}

fn render_cell(
    row: usize,
    column: usize,
    cell: TextCell,
    palette: TextPalette,
    target: RenderTarget<'_>,
) {
    let glyph_index = usize::from(cell.raw & 0x3F);
    let glyph_base = glyph_index * TEXT_CELL_HEIGHT;

    for y in 0..TEXT_CELL_HEIGHT {
        let bits = FONT_6847[glyph_base + y];
        let framebuffer_y = target.y_origin + row * TEXT_CELL_HEIGHT + y;
        for x in 0..TEXT_CELL_WIDTH {
            let lit = glyph_pixel(bits, x);
            let colour = if lit {
                palette.foreground
            } else {
                palette.background
            };
            let framebuffer_x = target.x_origin + column * TEXT_CELL_WIDTH + x;
            target.framebuffer[framebuffer_y * target.width + framebuffer_x] = colour;
        }
    }
}

fn glyph_pixel(bits: u8, x: usize) -> bool {
    if !(1..=6).contains(&x) {
        return false;
    }
    let bit = 6 - x;
    bits & (1 << bit) != 0
}

// MC6847 internal 64-character font, 12 rows per glyph.
// Derived from XRoar's generated `font-6847.c` reference data.
const FONT_6847: [u8; 64 * TEXT_CELL_HEIGHT] = [
    0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x1a, 0x2a, 0x2a, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x14, 0x22, 0x22, 0x3e, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x12, 0x12, 0x1c, 0x12,
    0x12, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x20, 0x20, 0x20, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3c, 0x12, 0x12, 0x12, 0x12, 0x12, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e,
    0x20, 0x20, 0x3c, 0x20, 0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x20, 0x20, 0x3c, 0x20,
    0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1e, 0x20, 0x20, 0x26, 0x22, 0x22, 0x1e, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x3e, 0x22, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x02, 0x02, 0x22,
    0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x24, 0x28, 0x30, 0x28, 0x24, 0x22, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x36, 0x2a, 0x2a, 0x22, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x32, 0x2a, 0x26, 0x22,
    0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x22, 0x22, 0x22, 0x22, 0x22, 0x3e, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3c, 0x22, 0x22, 0x3c, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x22, 0x22, 0x22, 0x2a, 0x24, 0x1a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x22, 0x22, 0x3c, 0x28,
    0x24, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x10, 0x08, 0x04, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x3e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x22, 0x22, 0x22, 0x22, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x14, 0x14,
    0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x22, 0x22, 0x2a, 0x2a, 0x36, 0x22, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x22, 0x22, 0x14, 0x08, 0x14, 0x22, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22,
    0x22, 0x14, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x02, 0x04, 0x08, 0x10,
    0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x38, 0x20, 0x20, 0x20, 0x20, 0x20, 0x38, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x20, 0x20, 0x10, 0x08, 0x04, 0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e,
    0x02, 0x02, 0x02, 0x02, 0x02, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x1c, 0x2a, 0x08, 0x08,
    0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x10, 0x3e, 0x10, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x08, 0x08, 0x08, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x14, 0x14, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x14, 0x36, 0x00, 0x36, 0x14, 0x14, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x08, 0x1e, 0x20, 0x1c, 0x02, 0x3c, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32,
    0x32, 0x04, 0x08, 0x10, 0x26, 0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x28, 0x28, 0x10, 0x2a,
    0x24, 0x1a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x08, 0x10, 0x20, 0x20, 0x20, 0x10, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x04, 0x02, 0x02, 0x02, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x1c, 0x3e, 0x1c,
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x3e, 0x08, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x30, 0x10, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x30, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x18, 0x24, 0x24, 0x24, 0x24, 0x24, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x18, 0x08, 0x08, 0x08, 0x08, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x1c, 0x20,
    0x20, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x22, 0x02, 0x0c, 0x02, 0x22, 0x1c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x04, 0x0c, 0x14, 0x3e, 0x04, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e,
    0x20, 0x3c, 0x02, 0x02, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x20, 0x20, 0x3c, 0x22,
    0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x1c, 0x22, 0x22, 0x1c, 0x22, 0x22, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c,
    0x22, 0x22, 0x1e, 0x02, 0x02, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x18,
    0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x08, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x04, 0x08, 0x10, 0x20, 0x10, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x3e, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x08, 0x04, 0x02, 0x04,
    0x08, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x24, 0x04, 0x08, 0x08, 0x00, 0x08, 0x00, 0x00,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_alphanumeric_codes() {
        assert_eq!(decode_text_byte(0x20).ch, ' ');
        assert_eq!(decode_text_byte(0x01).ch, 'A');
        assert_eq!(decode_text_byte(0x1A).ch, 'Z');
        assert_eq!(decode_text_byte(0x30).ch, '0');
        assert_eq!(decode_text_byte(0x39).ch, '9');
    }

    #[test]
    fn preserves_inverse_flag_without_changing_diagnostic_character() {
        let cell = decode_text_byte(0x41);

        assert_eq!(cell.ch, 'A');
        assert!(cell.inverse);
    }

    #[test]
    fn captures_32_by_16_text_screen() {
        let screen = TextScreen::capture(|index| if index == 33 { 0x02 } else { 0x20 });

        assert_eq!(screen.cell(1, 1).expect("cell should exist").ch, 'B');
        assert_eq!(screen.cell(16, 0), None);
        assert_eq!(screen.to_plain_text().lines().count(), TEXT_ROWS);
        assert_eq!(
            screen.to_plain_text().lines().next().expect("line"),
            " ".repeat(32)
        );
    }

    #[test]
    fn renders_text_screen_to_argb_framebuffer() {
        let screen = TextScreen::capture(|index| if index == 0 { 0x01 } else { 0x20 });
        let framebuffer = screen.render_argb(TextPalette::default());

        assert_eq!(framebuffer.len(), TEXT_FRAMEBUFFER_PIXELS);
        assert_eq!(framebuffer[0], DEFAULT_TEXT_BACKGROUND);
        assert_eq!(
            framebuffer[3 * TEXT_FRAMEBUFFER_WIDTH + 3],
            DEFAULT_TEXT_FOREGROUND
        );
    }

    #[test]
    fn renders_text_screen_with_visible_border() {
        let screen = TextScreen::capture(|index| if index == 0 { 0x01 } else { 0x20 });
        let framebuffer = screen.render_visible_argb(TextPalette::default());

        assert_eq!(framebuffer.len(), TEXT_VISIBLE_FRAMEBUFFER_PIXELS);
        assert_eq!(framebuffer[0], DEFAULT_TEXT_BORDER);
        assert_eq!(
            framebuffer
                [TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS],
            DEFAULT_TEXT_BACKGROUND
        );
        assert_eq!(
            framebuffer[(TEXT_TOP_BORDER_LINES + 3) * TEXT_VISIBLE_FRAMEBUFFER_WIDTH
                + TEXT_LEFT_BORDER_PIXELS
                + 3],
            DEFAULT_TEXT_FOREGROUND
        );
    }
}
