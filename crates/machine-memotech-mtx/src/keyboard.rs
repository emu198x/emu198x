//! Memotech MTX keyboard.
//!
//! The MTX keyboard is an 8x8 matrix. The active row is selected by writing
//! the row number to port $05 (bits 0-2), then reading from the same port
//! returns the column state for that row.
//!
//! A pressed key reads as 0 (active low).

/// Keyboard state: 8 rows of 8 keys each.
///
/// Each row byte uses bits 0-7 for keys (1 = pressed, for internal
/// storage). The `read()` method inverts for the active-low protocol.
pub struct KeyboardState {
    /// Row state. Index 0 = row 0, etc.
    /// Bits 0-7: 1 = key pressed (inverted on read).
    rows: [u8; 8],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self { rows: [0; 8] }
    }

    /// Set or clear a key. `row` is 0-7, `bit` is 0-7.
    pub fn set_key(&mut self, row: usize, bit: u8, pressed: bool) {
        if row < 8 && bit < 8 {
            if pressed {
                self.rows[row] |= 1 << bit;
            } else {
                self.rows[row] &= !(1 << bit);
            }
        }
    }

    /// Read the keyboard for the given row.
    ///
    /// Returns bits 0-7 (active low: 0 = pressed).
    #[must_use]
    pub fn read(&self, row: usize) -> u8 {
        if row < 8 { !self.rows[row] } else { 0xFF }
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
    }

    /// Raw row state (8 bytes, bits 0-7 per row, 1 = pressed).
    #[must_use]
    pub fn rows(&self) -> &[u8; 8] {
        &self.rows
    }

    /// Restore raw row state from a saved snapshot.
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
    fn no_keys_pressed() {
        let kbd = KeyboardState::new();
        for row in 0..8 {
            assert_eq!(kbd.read(row), 0xFF);
        }
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(3, 1, true); // Z
        assert_eq!(kbd.read(3) & 0x02, 0x00); // Active low
        assert_eq!(kbd.read(0), 0xFF); // Other row unaffected
    }

    #[test]
    fn release_key() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(3, 1, true);
        assert_eq!(kbd.read(3) & 0x02, 0x00);
        kbd.set_key(3, 1, false);
        assert_eq!(kbd.read(3) & 0x02, 0x02);
    }
}
