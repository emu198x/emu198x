//! Acorn Atom keyboard matrix.
//!
//! The Atom keyboard is scanned through the 8255 PPI: port A low nibble
//! drives a binary column index (0-9), port B reads that column's six row
//! lines. [`KeyboardState::read_row`] returns the pressed bits for one
//! column; the machine inverts them to the active-low port B value.

/// Keyboard state: 10 scanned rows × 6 columns, plus SHIFT and CTRL.
///
/// SHIFT and CTRL are not part of the scanned matrix — they are read on port B
/// bits 7 and 6, common to every column (Atom Technical Manual §25.5: PB6 = CTRL,
/// PB7 = SHIFT, both active-low). [`read_row`](Self::read_row) folds them into
/// every column's return value.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct KeyboardState {
    /// Matrix state. `matrix[row]` has bit `col` set when pressed.
    matrix: [u8; 10],
    /// SHIFT held (port B bit 7).
    shift: bool,
    /// CTRL held (port B bit 6).
    ctrl: bool,
}

/// Port B bit 7 — SHIFT.
const SHIFT_BIT: u8 = 0x80;
/// Port B bit 6 — CTRL.
const CTRL_BIT: u8 = 0x40;

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set or clear SHIFT (port B bit 7).
    pub fn set_shift(&mut self, pressed: bool) {
        self.shift = pressed;
    }

    /// Set or clear CTRL (port B bit 6).
    pub fn set_ctrl(&mut self, pressed: bool) {
        self.ctrl = pressed;
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
    /// MOS drives on 8255 port A). Returns the pressed bits (1 = pressed),
    /// with SHIFT (bit 7) and CTRL (bit 6) folded into every column; the caller
    /// inverts for the active-low port B read.
    #[must_use]
    pub fn read_row(&self, column: usize) -> u8 {
        let scanned = if column < 10 { self.matrix[column] } else { 0 };
        let modifiers =
            if self.shift { SHIFT_BIT } else { 0 } | if self.ctrl { CTRL_BIT } else { 0 };
        scanned | modifiers
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.matrix = [0; 10];
        self.shift = false;
        self.ctrl = false;
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
