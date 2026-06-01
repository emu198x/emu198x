//! PET keyboard matrix.
//!
//! 10x8 matrix, scanned through PIA/VIA. Column select on port A,
//! row read on port B.

/// Keyboard state: 10 rows x 8 columns.
pub struct KeyboardState {
    rows: [u8; 10],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self { rows: [0; 10] }
    }

    /// Set or clear a key.
    pub fn set_key(&mut self, row: usize, col: u8, pressed: bool) {
        if row < 10 && col < 8 {
            if pressed {
                self.rows[row] |= 1 << col;
            } else {
                self.rows[row] &= !(1 << col);
            }
        }
    }

    /// Scan the keyboard for the given column selection.
    ///
    /// Returns the OR of all selected columns, active low.
    #[must_use]
    pub fn read(&self, col_select: u8) -> u8 {
        let mut result = 0u8;
        for (row_idx, &row_val) in self.rows.iter().enumerate() {
            for col in 0..8 {
                if col_select & (1 << col) == 0 {
                    if row_val & (1 << col) != 0 {
                        result |= 1 << (row_idx & 7);
                    }
                }
            }
        }
        !result
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 10];
    }

    /// Raw row state.
    #[must_use]
    pub fn rows(&self) -> &[u8; 10] {
        &self.rows
    }

    /// Restore row state.
    pub fn set_rows(&mut self, rows: [u8; 10]) {
        self.rows = rows;
    }
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self::new()
    }
}
