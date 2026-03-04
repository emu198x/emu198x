//! C64 keyboard matrix.
//!
//! The C64 has an 8×8 keyboard matrix scanned via CIA1 ports A and B.
//! Port A ($DC00) selects which **column** to scan (active low output).
//! Port B ($DC01) reads which **rows** have a pressed key (active low input).

/// 8×8 keyboard matrix for the C64.
///
/// Internally indexed by column: `cols[c]` has bit `r` set when the key
/// at (row=r, col=c) is pressed. `scan()` takes the column-select mask
/// from CIA1 Port A and returns the row result for Port B.
pub struct KeyboardMatrix {
    /// Column state. `cols[c]` bit `r` = 1 means key (r, c) is pressed.
    cols: [u8; 8],
}

impl KeyboardMatrix {
    #[must_use]
    pub fn new() -> Self {
        Self { cols: [0; 8] }
    }

    /// Set or clear a key at the given row and column position.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row < 8 && col < 8 {
            if pressed {
                self.cols[col as usize] |= 1 << row;
            } else {
                self.cols[col as usize] &= !(1 << row);
            }
        }
    }

    /// Scan the keyboard matrix given a column-select mask from CIA1 Port A.
    ///
    /// `col_mask` is active low: a 0 bit selects that column for scanning.
    /// Returns active-low row data for Port B: a 0 bit means a key is
    /// pressed in one of the selected columns.
    #[must_use]
    pub fn scan(&self, col_mask: u8) -> u8 {
        let mut result: u8 = 0;
        for (col, &col_data) in self.cols.iter().enumerate() {
            if col_mask & (1 << col) == 0 {
                result |= col_data;
            }
        }
        // Invert: internally 1=pressed, CIA reads active-low (0=pressed)
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
        // Select all columns → no rows pressed
        assert_eq!(kbd.scan(0x00), 0xFF);
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardMatrix::new();
        // Press W at (row=1, col=1)
        kbd.set_key(1, 1, true);

        // Select column 1 only (bit 1 = 0, rest = 1) = 0xFD
        let result = kbd.scan(0xFD);
        assert_eq!(result & (1 << 1), 0); // Row 1 should be low (pressed)

        // Select column 0 only (bit 0 = 0) = 0xFE — W not in this column
        let result = kbd.scan(0xFE);
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn multiple_columns() {
        let mut kbd = KeyboardMatrix::new();
        kbd.set_key(0, 0, true); // Row 0, col 0 (DEL)
        kbd.set_key(3, 2, true); // Row 3, col 2 (6)

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
