//! C64 keyboard matrix.
//!
//! The C64 has an 8x8 keyboard matrix scanned via CIA1 ports A and B.
//! Port A ($DC00) outputs the row select (active low), and port B
//! ($DC01) reads the column result (active low: 0 = key pressed).

/// 8x8 keyboard matrix for the C64.
///
/// Internally stores 1 = pressed per bit. The `scan()` method returns
/// active-low column data as seen by CIA1 port B.
pub struct KeyboardMatrix {
    /// Row state. `rows[r]` has bit `c` set if key (row=r, col=c) is pressed.
    rows: [u8; 8],
}

impl KeyboardMatrix {
    #[must_use]
    pub fn new() -> Self {
        Self { rows: [0; 8] }
    }

    /// Set or clear a key at the given row and column position.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row < 8 && col < 8 {
            if pressed {
                self.rows[row as usize] |= 1 << col;
            } else {
                self.rows[row as usize] &= !(1 << col);
            }
        }
    }

    /// Scan the keyboard matrix given a row mask from CIA1 port A.
    ///
    /// `row_mask` is the value written to CIA1 port A (active low: a 0 bit
    /// selects that row for scanning). Returns active-low column data:
    /// a 0 bit means a key is pressed in one of the selected rows.
    #[must_use]
    pub fn scan(&self, row_mask: u8) -> u8 {
        let mut result: u8 = 0;
        for (row, &row_data) in self.rows.iter().enumerate() {
            // Active low: bit clear in row_mask means this row is selected
            if row_mask & (1 << row) == 0 {
                result |= row_data;
            }
        }
        // Invert: internally 1=pressed, but CIA reads active-low (0=pressed)
        !result
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
    }
}

impl Default for KeyboardMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_keys_pressed() {
        let kbd = KeyboardMatrix::new();
        // Select all rows (all bits 0)
        assert_eq!(kbd.scan(0x00), 0xFF);
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardMatrix::new();
        // Press key at row=1, col=1 (W key)
        kbd.set_key(1, 1, true);

        // Select row 1 only (bit 1 = 0, rest = 1) = 0xFD
        let result = kbd.scan(0xFD);
        assert_eq!(result & (1 << 1), 0); // Col 1 should be low (pressed)
        assert_eq!(result & !((1u8) << 1), 0xFD & !((1u8) << 1)); // Other cols high

        // Select row 0 only (bit 0 = 0) = 0xFE — key not visible
        let result = kbd.scan(0xFE);
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn multiple_rows() {
        let mut kbd = KeyboardMatrix::new();
        kbd.set_key(0, 0, true); // Row 0, col 0
        kbd.set_key(2, 3, true); // Row 2, col 3

        // Select rows 0 and 2: bits 0,2 = 0 → mask = 0xFA
        let result = kbd.scan(0xFA);
        assert_eq!(result & 0x01, 0x00); // Col 0 pressed (from row 0)
        assert_eq!(result & 0x08, 0x00); // Col 3 pressed (from row 2)
    }

    #[test]
    fn release_key() {
        let mut kbd = KeyboardMatrix::new();
        kbd.set_key(1, 1, true);
        assert_eq!(kbd.scan(0xFD) & 0x02, 0x00); // Pressed

        kbd.set_key(1, 1, false);
        assert_eq!(kbd.scan(0xFD) & 0x02, 0x02); // Released
    }

    #[test]
    fn release_all() {
        let mut kbd = KeyboardMatrix::new();
        kbd.set_key(0, 0, true);
        kbd.set_key(3, 5, true);
        kbd.release_all();
        assert_eq!(kbd.scan(0x00), 0xFF);
    }
}
