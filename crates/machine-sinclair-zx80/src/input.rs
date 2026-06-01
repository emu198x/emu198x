//! Input handling for the ZX80.
//!
//! Logical key names mapped to the 8x5 keyboard matrix. The ZX80 has the
//! same keyboard layout as the ZX81 -- 40 keys in an 8x5 matrix scanned
//! via port $FE.

/// Logical key on the ZX80 keyboard.
///
/// Each key maps to a (row, bit) pair in the 8x5 keyboard matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zx80Key {
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
    Newline,
    L,
    K,
    J,
    H,
    // Row 7 (addr bit A15)
    Space,
    Period,
    M,
    N,
    B,
}

impl Zx80Key {
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

            Self::Newline => (6, 0),
            Self::L => (6, 1),
            Self::K => (6, 2),
            Self::J => (6, 3),
            Self::H => (6, 4),

            Self::Space => (7, 0),
            Self::Period => (7, 1),
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
        assert_eq!(Zx80Key::Shift.matrix(), (0, 0));
        assert_eq!(Zx80Key::V.matrix(), (0, 4));
        assert_eq!(Zx80Key::A.matrix(), (1, 0));
        assert_eq!(Zx80Key::Newline.matrix(), (6, 0));
        assert_eq!(Zx80Key::Space.matrix(), (7, 0));
    }
}
