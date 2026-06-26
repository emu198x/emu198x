//! PET keyboard matrix.
//!
//! 10×8 matrix scanned through PIA #1. Port A drives a *binary* row
//! number 0-9 (the ten rows need four bits, so the select is encoded,
//! not one-hot); port B reads that row's eight column lines, active low
//! (a low bit means the key is pressed).

/// Keyboard state: 10 rows x 8 columns.
#[derive(serde::Serialize, serde::Deserialize)]
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

    /// Read the eight column lines for one binary-selected keyboard row.
    ///
    /// `row` is the value the PIA drives on port A (0-9); the return is
    /// the port B column read, active low (a low bit = key pressed).
    #[must_use]
    pub fn read_row(&self, row: u8) -> u8 {
        !self.rows.get(row as usize).copied().unwrap_or(0)
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
