//! Input handling for the Acorn Atom.
//!
//! The Atom has a 10x6 keyboard matrix scanned via the PIA. Port A drives
//! the columns (active low), port B reads the rows.

/// Logical key on the Acorn Atom keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomKey {
    // Row 0
    Shift,
    Ctrl,
    Space,
    // Row 1
    Escape,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    // Row 2
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    // Row 3
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    // Row 4
    X,
    Y,
    Z,
    Return,
    Delete,
    Copy,
    // Row 5
    N0,
    N1,
    N2,
    N3,
    N4,
    N5,
    N6,
    N7,
    // Row 6
    N8,
    N9,
    Colon,
    SemiColon,
    Comma,
    Minus,
    Period,
    Slash,
    // Row 7
    Up,
    Down,
    Left,
    Right,
    Lock,
}

impl AtomKey {
    /// Return the (row, column) pair for this key in the keyboard matrix.
    #[must_use]
    pub const fn matrix(self) -> (usize, usize) {
        match self {
            // Simplified 8x6 matrix mapping
            Self::N0 => (0, 0),
            Self::N1 => (0, 1),
            Self::N2 => (0, 2),
            Self::N3 => (0, 3),
            Self::N4 => (0, 4),
            Self::N5 => (0, 5),

            Self::N6 => (1, 0),
            Self::N7 => (1, 1),
            Self::N8 => (1, 2),
            Self::N9 => (1, 3),
            Self::Colon => (1, 4),
            Self::SemiColon => (1, 5),

            Self::Comma => (2, 0),
            Self::Minus => (2, 1),
            Self::Period => (2, 2),
            Self::Slash => (2, 3),
            Self::Copy => (2, 4),
            Self::Delete => (2, 5),

            Self::A => (3, 0),
            Self::B => (3, 1),
            Self::C => (3, 2),
            Self::D => (3, 3),
            Self::E => (3, 4),
            Self::F => (3, 5),

            Self::G => (4, 0),
            Self::H => (4, 1),
            Self::I => (4, 2),
            Self::J => (4, 3),
            Self::K => (4, 4),
            Self::L => (4, 5),

            Self::M => (5, 0),
            Self::N => (5, 1),
            Self::O => (5, 2),
            Self::P => (5, 3),
            Self::Q => (5, 4),
            Self::R => (5, 5),

            Self::S => (6, 0),
            Self::T => (6, 1),
            Self::U => (6, 2),
            Self::V => (6, 3),
            Self::W => (6, 4),
            Self::X => (6, 5),

            Self::Y => (7, 0),
            Self::Z => (7, 1),
            Self::Escape => (7, 2),
            Self::Return => (7, 3),
            Self::Space => (7, 4),
            Self::Lock => (7, 5),

            Self::Shift => (8, 0),
            Self::Ctrl => (8, 1),
            Self::Up => (8, 2),
            Self::Down => (8, 3),
            Self::Left => (8, 4),
            Self::Right => (8, 5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_valid() {
        let (r, c) = AtomKey::A.matrix();
        assert!(r < 10);
        assert!(c < 6);
    }
}
