//! The ZX81 ULA's keyboard matrix, shared with the ZX80.
//!
//! # What this crate used to be
//!
//! A display model as well: it counted T-states, raised NMI once a line, and
//! at each frame boundary read the `D_FILE` pointer from `$400C` and drew the
//! whole screen from the display file. Its own comment said what was wrong
//! with that — "a real ZX81 renders line-by-line during the NMI/HALT
//! bus-stealing cycle" — and #1032 is the issue that replaced it.
//!
//! The display file is not the display. Reading `$400C` assumes a well-formed
//! standard display file and shows nothing else, so pseudo-hi-res, WRX and
//! every other trick that makes the machine interesting were invisible; the
//! CPU paid nothing for the picture, so SLOW-mode timing was wrong; and a
//! frame appeared whether or not the CPU had generated one, which is how
//! every glyph came to be rendered from executable bytes without a test
//! noticing (#1030).
//!
//! Both machines now generate their pictures from the bus, in
//! `machine-sinclair-zx81`'s `video` module and `machine-sinclair-zx80`'s.
//! What is left here is the one piece they genuinely share.

/// Read the keyboard matrix.
///
/// Forty pressure-pad switches in an 8 x 5 matrix, the rows connected through
/// diodes to address lines A8-A15 — `reference/by-system/sinclair-zx81/`
/// §5. `addr_high` is the high byte of the port address, selecting rows by
/// active-low bits; `rows` is the 8-byte key state with 1 meaning pressed.
///
/// Returns bits 0-4 active low, bits 5-7 set. The ZX80 has the same matrix
/// and the same wiring, which is why this one function is shared where the
/// display is not.
#[must_use]
pub fn read_keyboard(addr_high: u8, rows: &[u8; 8]) -> u8 {
    let mut result: u8 = 0;
    for (i, &row) in rows.iter().enumerate() {
        if addr_high & (1 << i) == 0 {
            result |= row;
        }
    }
    !result & 0x1F | 0xE0
}

/// The keyboard matrix, kept as a type for the callers that name it.
pub struct Zx81Ula;

impl Zx81Ula {
    /// See [`read_keyboard`].
    #[must_use]
    pub fn read_keyboard(addr_high: u8, rows: &[u8; 8]) -> u8 {
        read_keyboard(addr_high, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_no_keys() {
        let rows = [0u8; 8];
        assert_eq!(read_keyboard(0x00, &rows), 0xFF);
    }

    #[test]
    fn keyboard_single_key() {
        let mut rows = [0u8; 8];
        rows[0] = 0x01; // Shift pressed
        // Scan row 0 (A8 = 0 → addr_high bit 0 clear)
        assert_eq!(read_keyboard(0xFE, &rows) & 0x1F, 0x1E);
        // Scan row 1 (A9 = 0) — shift not visible
        assert_eq!(read_keyboard(0xFD, &rows), 0xFF);
    }
}
