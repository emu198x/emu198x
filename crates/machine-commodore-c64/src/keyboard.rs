//! C64 keyboard matrix state.

use serde::{Deserialize, Serialize};

/// 8×8 keyboard matrix.
///
/// Internally indexed by column. Each column byte stores one bit per row,
/// where `1` means pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardMatrix {
    cols: [u8; 8],
}

impl KeyboardMatrix {
    /// Creates a cleared matrix.
    #[must_use]
    pub const fn new() -> Self {
        Self { cols: [0; 8] }
    }

    /// Sets or clears one key position.
    pub fn set_key(&mut self, row: u8, col: u8, pressed: bool) {
        if row >= 8 || col >= 8 {
            return;
        }

        let mask = 1u8 << row;
        let slot = &mut self.cols[usize::from(col)];
        if pressed {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
    }

    /// Returns the CIA1 Port B input value for one active-low column-select
    /// mask read from CIA1 Port A.
    #[must_use]
    pub fn scan(&self, col_mask: u8) -> u8 {
        let mut rows = 0u8;
        for (col, col_data) in self.cols.iter().enumerate() {
            if col_mask & (1u8 << col) == 0 {
                rows |= *col_data;
            }
        }
        !rows
    }

    /// Releases all keys.
    pub fn release_all(&mut self) {
        self.cols = [0; 8];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_matrix_reads_all_high() {
        let matrix = KeyboardMatrix::new();
        assert_eq!(matrix.scan(0x00), 0xFF);
    }

    #[test]
    fn selected_column_reads_pressed_row_low() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(1, 1, true);
        assert_eq!(matrix.scan(0xFD) & 0x02, 0x00);
        assert_eq!(matrix.scan(0xFE), 0xFF);
    }

    #[test]
    fn release_all_clears_columns() {
        let mut matrix = KeyboardMatrix::new();
        matrix.set_key(0, 0, true);
        matrix.set_key(3, 7, true);
        matrix.release_all();
        assert_eq!(matrix.scan(0x00), 0xFF);
    }
}
