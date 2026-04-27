//! Motorola MC6847 Video Display Generator helpers.
//!
//! This first slice models the alphanumeric text page shape used during Dragon
//! ROM bring-up. It deliberately exposes a text snapshot before pixel rendering
//! so we can validate the video RAM contents and SAM display-base wiring.

/// Alphanumeric text columns.
pub const TEXT_COLUMNS: usize = 32;
/// Alphanumeric text rows.
pub const TEXT_ROWS: usize = 16;
/// Bytes consumed by one MC6847 text screen.
pub const TEXT_SCREEN_BYTES: usize = TEXT_COLUMNS * TEXT_ROWS;

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
}
