//! Jupiter Ace keyboard.
//!
//! The keyboard is an 8x5 matrix, identical in scanning to the ZX Spectrum.
//! The high byte of the port address selects which half-rows to scan: each
//! bit (A8-A15) enables one half-row. Multiple rows can be scanned
//! simultaneously by clearing multiple address bits.
//!
//! # Half-row layout
//!
//! | Addr bit | Row | Keys (bit 0-4)                    |
//! |----------|-----|-----------------------------------|
//! | A8       | 0   | Shift, Z, X, C, V                |
//! | A9       | 1   | A, S, D, F, G                    |
//! | A10      | 2   | Q, W, E, R, T                    |
//! | A11      | 3   | 1, 2, 3, 4, 5                    |
//! | A12      | 4   | 0, 9, 8, 7, 6                    |
//! | A13      | 5   | P, O, I, U, Y                    |
//! | A14      | 6   | Enter, L, K, J, H                |
//! | A15      | 7   | Space, Symbol Shift, M, N, B     |
//!
//! A pressed key reads as 0 (active low). Bits 5-7 always read as 1.

/// Keyboard state: 8 half-rows of 5 keys each.
///
/// Each half-row byte uses bits 0-4 for keys (1 = pressed, for internal
/// storage). The `read()` method inverts and masks for the port $FE protocol.
pub struct KeyboardState {
    /// Half-row state. Index 0 = row 0 (Shift..V), etc.
    /// Bits 0-4: 1 = key pressed (inverted on read).
    rows: [u8; 8],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self { rows: [0; 8] }
    }

    /// Set or clear a key. `row` is 0-7, `bit` is 0-4.
    pub fn set_key(&mut self, row: usize, bit: u8, pressed: bool) {
        if row < 8 && bit < 5 {
            if pressed {
                self.rows[row] |= 1 << bit;
            } else {
                self.rows[row] &= !(1 << bit);
            }
        }
    }

    /// Read the keyboard for a port $FE access.
    ///
    /// `addr_high` is the high byte of the port address (bits A8-A15).
    /// Each cleared bit selects a half-row to scan.
    ///
    /// Returns bits 0-4 (active low: 0 = pressed), bits 5-7 = 1.
    #[must_use]
    pub fn read(&self, addr_high: u8) -> u8 {
        let mut result: u8 = 0x1F; // all keys released
        for row in 0..8u8 {
            if addr_high & (1 << row) == 0 {
                // This row is selected (active low addressing)
                result &= !self.rows[row as usize];
            }
        }
        result | 0xE0 // bits 5-7 always high
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
    }

    /// Raw half-row state (8 bytes, bits 0-4 per row, 1 = pressed).
    #[must_use]
    pub fn rows(&self) -> &[u8; 8] {
        &self.rows
    }

    /// Restore raw half-row state from a saved snapshot.
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
        // All rows selected, no keys down
        assert_eq!(kbd.read(0x00), 0xFF);
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(1, 0, true); // A
        // Select row 1 (A9 = bit 1 clear = 0xFD)
        assert_eq!(kbd.read(0xFD) & 0x1F, 0x1E);
        // Different row should be unaffected
        assert_eq!(kbd.read(0xFE), 0xFF);
    }

    #[test]
    fn release_key() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(1, 0, true);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x00);
        kbd.set_key(1, 0, false);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x01);
    }

    #[test]
    fn multiple_rows_scanned() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(0, 0, true); // Shift (row 0)
        kbd.set_key(4, 0, true); // 0 (row 4)
        // Scan both rows 0 and 4: bits 0 and 4 clear = 0xEE
        assert_eq!(kbd.read(0xEE) & 0x01, 0x00);
    }
}
