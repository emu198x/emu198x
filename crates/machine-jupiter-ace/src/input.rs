//! Input handling for the Jupiter Ace.
//!
//! Logical key names mapped to the 8x5 keyboard matrix. The matrix layout
//! is identical to the ZX Spectrum.

/// Logical key on the Jupiter Ace keyboard.
///
/// Each key maps to a (row, bit) pair in the 8x5 keyboard matrix.
/// The Ace has 40 keys in the same matrix layout as the Spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JupiterAceKey {
    // Row 0 (addr bit A8)
    Shift,
    Z,
    X,
    C,
    V,
    // Row 1 (addr bit A9)
    A,
    S,
    D,
    F,
    G,
    // Row 2 (addr bit A10)
    Q,
    W,
    E,
    R,
    T,
    // Row 3 (addr bit A11)
    N1,
    N2,
    N3,
    N4,
    N5,
    // Row 4 (addr bit A12)
    N0,
    N9,
    N8,
    N7,
    N6,
    // Row 5 (addr bit A13)
    P,
    O,
    I,
    U,
    Y,
    // Row 6 (addr bit A14)
    Enter,
    L,
    K,
    J,
    H,
    // Row 7 (addr bit A15)
    Space,
    SymbolShift,
    M,
    N,
    B,
}

impl JupiterAceKey {
    /// Return the (row, bit) pair for this key in the keyboard matrix.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            Self::Shift => (0, 0),
            // Symbol Shift sits beside Caps Shift on the Ace, not across the
            // keyboard where the Spectrum puts it. Getting this wrong shifted
            // the whole of both outer half-rows.
            Self::SymbolShift => (0, 1),
            Self::Z => (0, 2),
            Self::X => (0, 3),
            Self::C => (0, 4),

            Self::A => (1, 0),
            Self::S => (1, 1),
            Self::D => (1, 2),
            Self::F => (1, 3),
            Self::G => (1, 4),

            Self::Q => (2, 0),
            Self::W => (2, 1),
            Self::E => (2, 2),
            Self::R => (2, 3),
            Self::T => (2, 4),

            Self::N1 => (3, 0),
            Self::N2 => (3, 1),
            Self::N3 => (3, 2),
            Self::N4 => (3, 3),
            Self::N5 => (3, 4),

            Self::N0 => (4, 0),
            Self::N9 => (4, 1),
            Self::N8 => (4, 2),
            Self::N7 => (4, 3),
            Self::N6 => (4, 4),

            Self::P => (5, 0),
            Self::O => (5, 1),
            Self::I => (5, 2),
            Self::U => (5, 3),
            Self::Y => (5, 4),

            Self::Enter => (6, 0),
            Self::L => (6, 1),
            Self::K => (6, 2),
            Self::J => (6, 3),
            Self::H => (6, 4),

            Self::Space => (7, 0),
            Self::M => (7, 1),
            Self::N => (7, 2),
            Self::B => (7, 3),
            // V lives at the end of this half-row on the Ace; the Spectrum
            // has it at the end of the other one.
            Self::V => (7, 4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key on this keyboard, so the checks below see the whole matrix
    /// rather than whichever keys someone remembered.
    const ALL_KEYS: &[JupiterAceKey] = &[
        JupiterAceKey::Shift,
        JupiterAceKey::Z,
        JupiterAceKey::X,
        JupiterAceKey::C,
        JupiterAceKey::V,
        JupiterAceKey::A,
        JupiterAceKey::S,
        JupiterAceKey::D,
        JupiterAceKey::F,
        JupiterAceKey::G,
        JupiterAceKey::Q,
        JupiterAceKey::W,
        JupiterAceKey::E,
        JupiterAceKey::R,
        JupiterAceKey::T,
        JupiterAceKey::N1,
        JupiterAceKey::N2,
        JupiterAceKey::N3,
        JupiterAceKey::N4,
        JupiterAceKey::N5,
        JupiterAceKey::N0,
        JupiterAceKey::N9,
        JupiterAceKey::N8,
        JupiterAceKey::N7,
        JupiterAceKey::N6,
        JupiterAceKey::P,
        JupiterAceKey::O,
        JupiterAceKey::I,
        JupiterAceKey::U,
        JupiterAceKey::Y,
        JupiterAceKey::Enter,
        JupiterAceKey::L,
        JupiterAceKey::K,
        JupiterAceKey::J,
        JupiterAceKey::H,
        JupiterAceKey::Space,
        JupiterAceKey::SymbolShift,
        JupiterAceKey::M,
        JupiterAceKey::N,
        JupiterAceKey::B,
    ];

    #[test]
    fn key_matrix_rows() {
        assert_eq!(JupiterAceKey::Shift.matrix(), (0, 0));
        assert_eq!(JupiterAceKey::A.matrix(), (1, 0));
        assert_eq!(JupiterAceKey::Enter.matrix(), (6, 0));
        assert_eq!(JupiterAceKey::Space.matrix(), (7, 0));
    }

    /// The Ace is not a Spectrum, and this table was built as though it were.
    /// Symbol Shift sits beside Caps Shift at `(0, 1)` rather than across the
    /// keyboard, and `V` ends the *other* half-row — which pushed `Z X C` and
    /// `M N B` one cell each. Seven letter keys therefore typed their
    /// left-hand neighbour: `B` produced `V`, `C` produced `X`, Symbol Shift
    /// produced `M`, and `Z` produced nothing. Nothing errored; the machine
    /// just typed the wrong letter.
    ///
    /// The previous version of `key_matrix_rows` asserted the two cells this
    /// got wrong, so the bug had a test holding it in place. Cross-checked
    /// against MAME `cantab/jupace.cpp`, whose `KEY0` bit 1 is named "Symbol
    /// Shift" outright, and confirmed by typing the alphabet on the real ROM.
    #[test]
    fn the_outer_half_rows_are_the_ace_layout_not_the_spectrum() {
        assert_eq!(JupiterAceKey::Shift.matrix(), (0, 0));
        assert_eq!(JupiterAceKey::SymbolShift.matrix(), (0, 1));
        assert_eq!(JupiterAceKey::Z.matrix(), (0, 2));
        assert_eq!(JupiterAceKey::X.matrix(), (0, 3));
        assert_eq!(JupiterAceKey::C.matrix(), (0, 4));

        assert_eq!(JupiterAceKey::Space.matrix(), (7, 0));
        assert_eq!(JupiterAceKey::M.matrix(), (7, 1));
        assert_eq!(JupiterAceKey::N.matrix(), (7, 2));
        assert_eq!(JupiterAceKey::B.matrix(), (7, 3));
        assert_eq!(JupiterAceKey::V.matrix(), (7, 4));
    }

    /// Two keys sharing a cell is silent — the later one simply presses the
    /// earlier one and the machine types the wrong character.
    #[test]
    fn no_two_keys_share_a_matrix_cell() {
        let mut seen: Vec<((usize, u8), JupiterAceKey)> = Vec::new();
        for &key in ALL_KEYS {
            let cell = key.matrix();
            assert!(cell.0 < 8 && cell.1 < 5, "{key:?} is outside the matrix");
            if let Some((_, other)) = seen.iter().find(|(c, _)| *c == cell) {
                panic!("{key:?} and {other:?} both claim cell {cell:?}");
            }
            seen.push((cell, key));
        }
    }

    /// The Ace drives all forty cells — eight half-rows of five. An empty one
    /// means a key is missing, which is invisible: it does not fail, it just
    /// cannot be pressed.
    #[test]
    fn every_cell_is_claimed() {
        let mut filled = [[false; 5]; 8];
        for &key in ALL_KEYS {
            let (r, c) = key.matrix();
            filled[r][usize::from(c)] = true;
        }
        for (r, row) in filled.iter().enumerate() {
            for (c, claimed) in row.iter().enumerate() {
                assert!(claimed, "cell ({r}, {c}) has no key");
            }
        }
    }
}
