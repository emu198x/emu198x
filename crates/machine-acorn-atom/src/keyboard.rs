//! Acorn Atom keyboard matrix.
//!
//! The Atom keyboard is scanned through the 8255 PPI: port A low nibble
//! drives a binary column index (0-9), port B reads that column's six row
//! lines. [`KeyboardState::read_row`] returns the pressed bits for one
//! column; the machine inverts them to the active-low port B value.

/// Keyboard state: 10 rows x 6 columns.
#[derive(serde::Serialize, serde::Deserialize)]
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

    /// Read the row lines for one keyboard column (the binary index the
    /// MOS drives on 8255 port A). Returns the pressed bits (1 = pressed);
    /// the caller inverts for the active-low port B read.
    #[must_use]
    pub fn read_row(&self, column: usize) -> u8 {
        if column < 10 { self.matrix[column] } else { 0 }
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
