//! C64 keyboard matrix.
//!
//! The C64 has an 8x8 keyboard matrix scanned via CIA1 ports A and B.
//! Port A ($DC00) outputs the column select (active low), and port B
//! ($DC01) reads the row result (active low: 0 = key pressed).

/// 8x8 keyboard matrix for the C64.
///
/// Internally stores 1 = pressed per bit. The `scan()` method returns
/// active-low row data as seen by CIA1 port B.
pub struct KeyboardMatrix {
    /// Column state. `cols[c]` has bit `r` set if key (col=c, row=r) is pressed.
    cols: [u8; 8],
}

impl KeyboardMatrix {
    #[must_use]
    pub fn new() -> Self {
        Self { cols: [0; 8] }
    }

    /// Set or clear a key at the given column and row position.
    pub fn set_key(&mut self, col: u8, row: u8, pressed: bool) {
        if col < 8 && row < 8 {
            if pressed {
                self.cols[col as usize] |= 1 << row;
            } else {
                self.cols[col as usize] &= !(1 << row);
            }
        }
    }

    /// Scan the keyboard matrix given a column mask from CIA1 port A.
    ///
    /// `col_mask` is the value written to CIA1 port A (active low: a 0 bit
    /// selects that column for scanning). Returns active-low row data:
    /// a 0 bit means a key is pressed in one of the selected columns.
    #[must_use]
    pub fn scan(&self, col_mask: u8) -> u8 {
        let mut result: u8 = 0;
        for (col, &col_data) in self.cols.iter().enumerate() {
            // Active low: bit clear in col_mask means this column is selected
            if col_mask & (1 << col) == 0 {
                result |= col_data;
            }
        }
        // Invert: internally 1=pressed, but CIA reads active-low (0=pressed)
        !result
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.cols = [0; 8];
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
        // Select all columns (all bits 0)
        assert_eq!(kbd.scan(0x00), 0xFF);
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardMatrix::new();
        // Press key at col=1, row=1 (W key)
        kbd.set_key(1, 1, true);

        // Select column 1 only (bit 1 = 0, rest = 1) = 0xFD
        let result = kbd.scan(0xFD);
        assert_eq!(result & (1 << 1), 0); // Row 1 should be low (pressed)
        assert_eq!(result & !((1u8) << 1), 0xFD & !((1u8) << 1)); // Other rows high

        // Select column 0 only (bit 0 = 0) = 0xFE — key not visible
        let result = kbd.scan(0xFE);
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn multiple_columns() {
        let mut kbd = KeyboardMatrix::new();
        kbd.set_key(0, 0, true); // Col 0, row 0
        kbd.set_key(2, 3, true); // Col 2, row 3

        // Select columns 0 and 2: bits 0,2 = 0 → mask = 0xFA
        let result = kbd.scan(0xFA);
        assert_eq!(result & 0x01, 0x00); // Row 0 pressed (from col 0)
        assert_eq!(result & 0x08, 0x00); // Row 3 pressed (from col 2)
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
