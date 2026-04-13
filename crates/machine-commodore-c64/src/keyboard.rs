//! C64 keyboard matrix state.

use serde::{Deserialize, Serialize};

/// 8×8 keyboard matrix.
///
/// Internally indexed by row. Each row byte stores one bit per column, where
/// `1` means pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardMatrix {
    rows: [u8; 8],
}

impl KeyboardMatrix {
    /// Creates a cleared matrix.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: [0; 8] }
    }

    /// Sets or clears one key position.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row >= 8 || col >= 8 {
            return;
        }

        let mask = 1u8 << col;
        let slot = &mut self.rows[usize::from(row)];
        if pressed {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
    }

    /// Returns the CIA1 Port B input value for one active-low row-select mask
    /// driven through CIA1 Port A.
    #[must_use]
    pub fn scan(&self, row_mask: u8) -> u8 {
        let mut cols = 0u8;
        for (row, row_data) in self.rows.iter().enumerate() {
            if row_mask & (1u8 << row) == 0 {
                cols |= *row_data;
            }
        }
        !cols
    }

    /// Releases all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_matrix_reads_all_high() {
        let matrix = KeyboardMatrix::new();
        assert_eq!(matrix.scan(0x00), 0xFF);
        assert_eq!(matrix.scan(0xFF), 0xFF);
    }

    #[test]
    fn selected_row_reads_pressed_column_low() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 1, true);
        assert_eq!(matrix.scan(0xFE) & 0x02, 0x00);
        assert_eq!(matrix.scan(0xFD), 0xFF);
    }

    #[test]
    fn other_rows_remain_high_for_same_column() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 1, true);
        assert_eq!(matrix.scan(0xFD), 0xFF);
    }

    #[test]
    fn release_all_clears_rows() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 0, true);
        matrix.set_key(3, 7, true);
        matrix.release_all();
        assert_eq!(matrix.scan(0x00), 0xFF);
    }
}
