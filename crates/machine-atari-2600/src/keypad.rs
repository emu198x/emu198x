//! Atari keypad / keyboard controller (CX-21, CX-23) — a 4×3 key matrix.
//!
//! Layout, columns left→right, rows top→bottom (Stella `Keyboard.cxx`):
//!
//! ```text
//!          col0  col1  col2
//!   row0:   1     2     3
//!   row1:   4     5     6
//!   row2:   7     8     9
//!   row3:   *     0     #
//! ```
//!
//! The four matrix rows are wired to the joystick's four direction output pins
//! (the port's SWCHA nibble). To scan, the kernel sets those pins as outputs
//! and pulls one row low at a time, then reads the columns: columns 0 and 1 on
//! the analog INPT lines (INPT0/INPT1 for the left jack, INPT2/INPT3 for the
//! right) and column 2 on the digital fire line (INPT4 / INPT5). A pressed key
//! grounds its column only while *its own* row is the one driven low — that is
//! how the kernel tells which key in the column is down.

/// One keypad controller's pressed-key matrix.
#[derive(Default, Clone, Copy)]
pub(crate) struct Keypad {
    /// `pressed[row][col]`, row 0-3 (top→bottom), col 0-2 (left→right).
    pressed: [[bool; 3]; 4],
}

impl Keypad {
    /// Press or release a matrix key. Out-of-range row/col is ignored.
    pub(crate) fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if let Some(r) = self.pressed.get_mut(row as usize)
            && let Some(cell) = r.get_mut(col as usize)
        {
            *cell = pressed;
        }
    }

    /// Which columns are grounded, given the port's 4-bit row drive (`rows`
    /// bit `r` = row `r`'s pin level; a low bit is the scanned row). A column
    /// grounds when a pressed key in it sits on a scanned (low) row.
    pub(crate) fn columns_grounded(&self, rows: u8) -> [bool; 3] {
        let mut gnd = [false; 3];
        for (r, keys) in self.pressed.iter().enumerate() {
            if (rows >> r) & 1 == 0 {
                for (c, &pressed) in keys.iter().enumerate() {
                    gnd[c] |= pressed;
                }
            }
        }
        gnd
    }
}

#[cfg(test)]
mod tests {
    use super::Keypad;

    #[test]
    fn a_key_grounds_its_column_only_on_its_scanned_row() {
        let mut kp = Keypad::default();
        kp.set_key(1, 2, true); // key "6": row 1, col 2

        // Row 1 driven low (bit 1 = 0), others high → col 2 grounds.
        assert_eq!(kp.columns_grounded(0b1101), [false, false, true]);
        // Row 0 driven low instead → key 6 not on this row, nothing grounds.
        assert_eq!(kp.columns_grounded(0b1110), [false, false, false]);
        // No row driven low → nothing grounds.
        assert_eq!(kp.columns_grounded(0b1111), [false, false, false]);
    }

    #[test]
    fn unpressed_matrix_never_grounds() {
        let kp = Keypad::default();
        for rows in 0..=0x0Fu8 {
            assert_eq!(kp.columns_grounded(rows), [false, false, false]);
        }
    }
}
