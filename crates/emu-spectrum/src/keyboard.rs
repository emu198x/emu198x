//! ZX Spectrum keyboard.
//!
//! The Spectrum keyboard is an 8×5 matrix of half-rows, read via port $FE.
//! The high byte of the port address selects which half-rows to scan: each
//! bit (A8-A15) enables one half-row. Multiple rows can be scanned
//! simultaneously by clearing multiple address bits.
//!
//! # Half-row layout
//!
//! | Addr bit | Row | Keys (bit 0-4)                |
//! |----------|-----|-------------------------------|
//! | A8       | 0   | Shift, Z, X, C, V            |
//! | A9       | 1   | A, S, D, F, G                |
//! | A10      | 2   | Q, W, E, R, T                |
//! | A11      | 3   | 1, 2, 3, 4, 5                |
//! | A12      | 4   | 0, 9, 8, 7, 6                |
//! | A13      | 5   | P, O, I, U, Y                |
//! | A14      | 6   | Enter, L, K, J, H            |
//! | A15      | 7   | Space, Sym, M, N, B          |
//!
//! A pressed key reads as 0 (active low). Bits 5-7 always read as 1.

/// Keyboard state: 8 half-rows of 5 keys each.
///
/// Each half-row byte uses bits 0-4 for keys (1 = pressed, for internal
/// storage). The `read()` method inverts and masks for the port $FE protocol.
pub struct KeyboardState {
    /// Half-row state. Index 0 = row 0 (Shift..V), etc.
    /// Bits 0-4: 1 = key pressed (inverted on read for port $FE).
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
    /// Each cleared bit selects a half-row to scan. Multiple rows are OR'd
    /// together (any key pressed in any selected row reads as 0).
    ///
    /// The Spectrum keyboard is a passive 8×5 matrix. When multiple keys are
    /// pressed, current can flow through the switch network and pull extra
    /// columns low — "ghosting". This implementation computes the transitive
    /// closure: any row reachable through shared columns with the selected
    /// rows contributes its columns to the result.
    ///
    /// Returns bits 0-4 (active low: 0 = pressed), bits 5-7 = 1.
    #[must_use]
    pub fn read(&self, addr_high: u8) -> u8 {
        // Start with directly selected rows
        let mut active_rows: u8 = !addr_high;

        // Propagate through shared columns until stable
        loop {
            // Columns reachable from any active row
            let mut cols: u8 = 0;
            for i in 0..8 {
                if active_rows & (1 << i) != 0 {
                    cols |= self.rows[i];
                }
            }

            // Rows reachable from those columns
            let mut new_rows = active_rows;
            for i in 0..8 {
                if self.rows[i] & cols != 0 {
                    new_rows |= 1 << i;
                }
            }

            if new_rows == active_rows {
                break;
            }
            active_rows = new_rows;
        }

        // Final result: all columns from all reachable rows
        let mut result: u8 = 0;
        for i in 0..8 {
            if active_rows & (1 << i) != 0 {
                result |= self.rows[i];
            }
        }
        (!result & 0x1F) | 0xE0
    }

    /// Release all keys.
    pub fn release_all(&mut self) {
        self.rows = [0; 8];
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
        // All rows selected (addr high = 0x00)
        assert_eq!(kbd.read(0x00), 0xFF);
    }

    #[test]
    fn single_key_pressed() {
        let mut kbd = KeyboardState::new();
        // Press 'A' (row 1, bit 0)
        kbd.set_key(1, 0, true);

        // Read row 1 (A9 = 0, others = 1) → addr_high = 0b1111_1101 = 0xFD
        let result = kbd.read(0xFD);
        assert_eq!(result & 0x1F, 0x1E); // Bit 0 clear (A pressed)

        // Read a different row → key not visible
        let result = kbd.read(0xFE); // Row 0 only
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn multiple_rows() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(0, 0, true); // Shift
        kbd.set_key(4, 0, true); // 0

        // Select both rows: A8=0, A12=0 → addr_high = 0b1110_1110 = 0xEE
        let result = kbd.read(0xEE);
        assert_eq!(result & 0x1F, 0x1E); // Bit 0 clear from both rows
    }

    #[test]
    fn release_key() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(1, 0, true);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x00); // Pressed (active low)

        kbd.set_key(1, 0, false);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x01); // Released
    }

    #[test]
    fn bits_5_7_always_high() {
        let kbd = KeyboardState::new();
        assert_eq!(kbd.read(0x00) & 0xE0, 0xE0);
    }

    #[test]
    fn ghost_three_corners_produces_fourth() {
        let mut kbd = KeyboardState::new();
        // Press Shift (row 0, col 0), Z (row 0, col 1), A (row 1, col 0)
        kbd.set_key(0, 0, true);
        kbd.set_key(0, 1, true);
        kbd.set_key(1, 0, true);

        // Read row 1 only (A9=0, rest=1 → 0xFD)
        // Without ghosting: only col 0 (A) would be active.
        // With ghosting: row 0 and row 1 share col 0, so row 0's col 1
        // propagates → col 1 (S) appears ghosted.
        let result = kbd.read(0xFD);
        assert_eq!(
            result & 0x1F,
            0x1C, // cols 0 and 1 active (bits 0,1 = 0)
            "S (row 1, col 1) should be ghosted"
        );
    }

    #[test]
    fn no_ghost_with_two_keys_no_shared_axis() {
        let mut kbd = KeyboardState::new();
        // Press (row 0, col 0) and (row 1, col 1) — diagonal, no shared row/column
        kbd.set_key(0, 0, true);
        kbd.set_key(1, 1, true);

        // Read row 0 only: should see only col 0
        assert_eq!(kbd.read(0xFE) & 0x1F, 0x1E);
        // Read row 1 only: should see only col 1
        assert_eq!(kbd.read(0xFD) & 0x1F, 0x1D);
    }

    #[test]
    fn ghost_propagates_transitively() {
        let mut kbd = KeyboardState::new();
        // Chain: row 0 col 0, row 0 col 1, row 1 col 1, row 1 col 2
        // Row 0 and row 1 share col 1.
        // Now add row 2 col 2 — row 1 and row 2 share col 2.
        // Reading row 2 should propagate: row 2 → col 2 → row 1 → col 1 → row 0 → col 0
        kbd.set_key(0, 0, true); // row 0, col 0
        kbd.set_key(0, 1, true); // row 0, col 1
        kbd.set_key(1, 1, true); // row 1, col 1
        kbd.set_key(1, 2, true); // row 1, col 2
        kbd.set_key(2, 2, true); // row 2, col 2

        // Read row 2 only (A10=0 → 0xFB)
        // Transitive: row 2 → col 2 → row 1 (shares col 2) → col 1 → row 0 (shares col 1) → col 0
        // All three columns should be active
        let result = kbd.read(0xFB);
        assert_eq!(
            result & 0x1F,
            0x18, // cols 0, 1, 2 active (bits 0,1,2 = 0)
            "ghost should propagate transitively through row 1 to row 0"
        );
    }
}
