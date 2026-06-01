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
            Self::Z => (0, 1),
            Self::X => (0, 2),
            Self::C => (0, 3),
            Self::V => (0, 4),

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
            Self::SymbolShift => (7, 1),
            Self::M => (7, 2),
            Self::N => (7, 3),
            Self::B => (7, 4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_rows() {
        assert_eq!(JupiterAceKey::Shift.matrix(), (0, 0));
        assert_eq!(JupiterAceKey::V.matrix(), (0, 4));
        assert_eq!(JupiterAceKey::A.matrix(), (1, 0));
        assert_eq!(JupiterAceKey::Enter.matrix(), (6, 0));
        assert_eq!(JupiterAceKey::Space.matrix(), (7, 0));
        assert_eq!(JupiterAceKey::SymbolShift.matrix(), (7, 1));
    }
}
