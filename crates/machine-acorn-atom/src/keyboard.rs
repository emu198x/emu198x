//! Acorn Atom keyboard matrix.
//!
//! The Atom keyboard is scanned through the PIA. Port A low nibble selects
//! the column (active low), port B reads the row state.

/// Keyboard state: 10 rows x 6 columns.
pub struct KeyboardState {
    /// Matrix state. `matrix[row]` has bit `col` set when pressed.
    matrix: [u8; 10],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self { matrix: [0; 10] }
    }

    /// Set or clear a key.
    pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
        if row < 10 && col < 6 {
            if pressed {
                self.matrix[row] |= 1 << col;
            } else {
                self.matrix[row] &= !(1 << col);
            }
        }
    }

    /// Read keyboard for the given column selection from PIA port A.
    ///
    /// `col_select` is the PIA port A low nibble, active low. Each cleared
    /// bit selects a column. Returns the OR of all selected columns across
    /// all rows as a row mask. Bit set = key pressed.
    #[must_use]
    pub fn read(&self, col_select: u8) -> u8 {
        let mut result = 0u8;
        for row in 0..10 {
            for col in 0..6 {
                if col_select & (1 << col) == 0 {
                    // Column selected (active low)
                    if self.matrix[row] & (1 << col) != 0 {
                        result |= 1 << (row & 7);
                    }
                }
            }
        }
        !result // Active low output
    }

    /// Read a specific row.
    ///
    /// Returns the row state as a bitmap (1 = pressed).
    #[must_use]
    pub fn read_row(&self, row: usize) -> u8 {
        if row < 10 { self.matrix[row] } else { 0 }
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.matrix = [0; 10];
    }

    /// Raw matrix state.
    #[must_use]
    pub fn matrix(&self) -> &[u8; 10] {
        &self.matrix
    }

    /// Restore matrix state from a saved snapshot.
    pub fn set_matrix(&mut self, matrix: [u8; 10]) {
        self.matrix = matrix;
    }
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_keys_pressed() {
        let kbd = KeyboardState::new();
        assert_eq!(kbd.read_row(0), 0);
    }

    #[test]
    fn single_key() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(3, 0, true); // 'A'
        assert_eq!(kbd.read_row(3) & 0x01, 0x01);
        kbd.set_key(3, 0, false);
        assert_eq!(kbd.read_row(3) & 0x01, 0x00);
    }
}
