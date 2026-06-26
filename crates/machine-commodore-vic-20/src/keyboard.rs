//! VIC-20 keyboard matrix.
//!
//! 8x8 matrix, scanned through VIA#1. Port A selects columns (active low),
//! port B reads rows.

/// Keyboard state: 8 rows x 8 columns.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct KeyboardState {
    /// Matrix state. `rows[row]` has bit `col` set when pressed.
    rows: [u8; 8],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self { rows: [0; 8] }
    }

    /// Set or clear a key.
    pub fn set_key(&mut self, row: usize, col: u8, pressed: bool) {
        if row < 8 && col < 8 {
            if pressed {
                self.rows[row] |= 1 << col;
            } else {
                self.rows[row] &= !(1 << col);
            }
        }
    }

    /// Read the keyboard matrix for the given column selection.
    ///
    /// `col_select` is the VIA port A output (active low). Returns
    /// the OR of selected columns across all rows (active low).
    #[must_use]
    pub fn read(&self, col_select: u8) -> u8 {
        let mut result = 0u8;
        for (row_idx, &row_val) in self.rows.iter().enumerate() {
            for col in 0..8 {
                if col_select & (1 << col) == 0 {
                    // Column selected (active low)
                    if row_val & (1 << col) != 0 {
                        result |= 1 << row_idx;
                    }
                }
            }
        }
        !result // Active low output
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
    }

    /// Raw row state.
    #[must_use]
    pub fn rows(&self) -> &[u8; 8] {
        &self.rows
    }

    /// Restore row state from a saved snapshot.
    pub fn set_rows(&mut self, rows: [u8; 8]) {
        self.rows = rows;
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
    fn no_keys_reads_ff() {
        let kbd = KeyboardState::new();
        assert_eq!(kbd.read(0x00), 0xFF);
    }

    #[test]
    fn single_key() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(2, 1, true); // A key
        assert_ne!(kbd.read(0xFD), 0xFF); // Column 1 selected
    }
}
