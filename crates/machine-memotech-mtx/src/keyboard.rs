//! Memotech MTX keyboard — drive/sense model.
//!
//! Modelled on MEMU's `kbd2.c`. The CPU writes a **drive** byte to port `$05`
//! (`kbd_out5`); each zero bit drives one of eight matrix columns (active
//! low). Reading port `$05` (`kbd_in5`, "Sense1") returns the AND of the
//! sense lines of every driven column, low 8 bits. Reading port `$06`
//! (`kbd_in6`, "Sense2") returns the two extra sense rows (bits 8-9, shifted
//! down to bits 0-1) OR'd with the **country code** in bits 2-3 (00 =
//! English). A pressed key pulls its sense bit low.

/// Country code reported in `kbd_in6` bits 2-3. English = 0.
const COUNTRY_ENGLISH: u8 = 0x00;

/// Keyboard matrix: eight drive columns, each with ten sense lines
/// (bits 0-9). A clear bit means the key is held (active low).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct KeyboardState {
    sense: [u16; 8],
    /// Joystick overlay, ANDed into the sense read. The MTX joysticks share
    /// the keyboard matrix lines, so a pressed direction pulls the same sense
    /// bit low as a key would (MAME `mtx_key_lo_r` ANDs the joystick port into
    /// the matrix). Idle is all-high. Kept separate from `sense` so joystick
    /// and key state don't clobber each other.
    joystick: [u16; 8],
}

impl KeyboardState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sense: [0x03FF; 8],
            joystick: [0x03FF; 8],
        }
    }

    /// Set or clear a key. `col` is the drive column 0-7, `bit` the sense
    /// line 0-9 (8-9 are read back through port `$06`).
    pub fn set_key(&mut self, col: usize, bit: u8, pressed: bool) {
        if col < 8 && bit < 10 {
            if pressed {
                self.sense[col] &= !(1 << bit);
            } else {
                self.sense[col] |= 1 << bit;
            }
        }
    }

    /// Set or clear a joystick line at matrix position `(col, bit)`. Same
    /// active-low convention as [`Self::set_key`], on the separate overlay.
    pub fn set_joystick_bit(&mut self, col: usize, bit: u8, pressed: bool) {
        if col < 8 && bit < 10 {
            if pressed {
                self.joystick[col] &= !(1 << bit);
            } else {
                self.joystick[col] |= 1 << bit;
            }
        }
    }

    /// Combined sense for a driven column: keys AND joystick.
    fn line(&self, col: usize) -> u16 {
        self.sense[col] & self.joystick[col]
    }

    /// Port `$05` read ("Sense1"): low 8 sense bits, ANDed over every column
    /// the `drive` byte is currently driving (a zero bit drives the column).
    /// `$FF` when nothing is held.
    #[must_use]
    pub fn in5(&self, drive: u8) -> u8 {
        let mut result: u16 = 0x00FF;
        for i in 0..8 {
            if drive & (1 << i) == 0 {
                result &= self.line(i);
            }
        }
        (result & 0xFF) as u8
    }

    /// Port `$06` read ("Sense2"): the two high sense rows (bits 8-9 shifted
    /// to bits 0-1) OR'd with the country code in bits 2-3. `0x03` when
    /// nothing is held on an English machine.
    #[must_use]
    pub fn in6(&self, drive: u8) -> u8 {
        let mut result: u16 = 0x03FF;
        for i in 0..8 {
            if drive & (1 << i) == 0 {
                result &= self.line(i);
            }
        }
        ((result >> 8) & 0x03) as u8 | COUNTRY_ENGLISH
    }

    /// Release all keys and joystick lines.
    pub fn release_all(&mut self) {
        self.sense = [0x03FF; 8];
        self.joystick = [0x03FF; 8];
    }

    /// Raw sense state (for snapshots).
    #[must_use]
    pub fn sense(&self) -> &[u16; 8] {
        &self.sense
    }

    /// Restore raw sense state from a saved snapshot.
    pub fn set_sense(&mut self, sense: [u16; 8]) {
        self.sense = sense;
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
        // Drive every column (drive = 0): nothing held reads all-high.
        assert_eq!(kbd.in5(0x00), 0xFF);
        assert_eq!(kbd.in6(0x00), 0x03); // bits 0-1 high, country English
    }

    #[test]
    fn single_key_pulls_sense_low() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(3, 1, true); // column 3, sense line 1
        // Driving only column 3 (drive bit 3 = 0, rest high) senses it.
        assert_eq!(kbd.in5(!(1 << 3)) & 0x02, 0x00);
        // Driving a different column does not.
        assert_eq!(kbd.in5(!(1 << 0)) & 0x02, 0x02);
    }

    #[test]
    fn extra_rows_read_through_port6() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(2, 8, true); // sense line 8 → port $06 bit 0
        assert_eq!(kbd.in6(!(1 << 2)) & 0x01, 0x00);
        assert_eq!(kbd.in6(0x00) & 0x0C, COUNTRY_ENGLISH); // country preserved
    }

    #[test]
    fn release_restores_high() {
        let mut kbd = KeyboardState::new();
        kbd.set_key(3, 1, true);
        assert_eq!(kbd.in5(!(1 << 3)) & 0x02, 0x00);
        kbd.set_key(3, 1, false);
        assert_eq!(kbd.in5(!(1 << 3)) & 0x02, 0x02);
    }

    #[test]
    fn joystick_overlay_pulls_sense_low_independently_of_keys() {
        let mut kbd = KeyboardState::new();
        // P1 up sits at column 2, sense bit 7 (read low byte via $05).
        kbd.set_joystick_bit(2, 7, true);
        assert_eq!(kbd.in5(!(1 << 2)) & 0x80, 0x00, "joystick pulls bit 7 low");
        // A key on the same column, different bit, is unaffected.
        assert_eq!(kbd.in5(!(1 << 2)) & 0x01, 0x01);
        // P2 fire sits at column 7, sense bit 8 (read through $06 bit 0).
        kbd.set_joystick_bit(7, 8, true);
        assert_eq!(kbd.in6(!(1 << 7)) & 0x01, 0x00, "P2 fire on high sense row");
        // Release leaves the keyboard sense untouched.
        kbd.set_joystick_bit(2, 7, false);
        assert_eq!(kbd.in5(!(1 << 2)) & 0x80, 0x80);
    }
}
