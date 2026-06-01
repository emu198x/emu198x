//! Input handling for the Memotech MTX.
//!
//! The MTX has an 8x8 keyboard matrix read via I/O port $05.
//! The active row is selected by writing to port $05, then
//! reading from the same port.

/// Logical key on the MTX keyboard.
///
/// Each key maps to a (row, bit) pair in the 8x8 keyboard matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MtxKey {
    // Row 0
    N1, N2, N3, N4, N5, N6, N7, N8,
    // Row 1
    N9, N0, Minus, Equal, Backslash, BracketLeft, BracketRight, Semicolon,
    // Row 2
    Quote, Comma, Period, Slash, Dead, Pound, Delete, CtrlLeft,
    // Row 3
    Shift, Z, X, C, V, B, N, M,
    // Row 4
    A, S, D, F, G, H, J, K,
    // Row 5
    L, Enter, Q, W, E, R, T, Y,
    // Row 6
    U, I, O, P, Escape, Tab, CapsLock, Space,
    // Row 7
    F1, F2, F3, F4, F5, Left, Right, Up,
}

impl MtxKey {
    /// Return the (row, bit) pair for this key in the keyboard matrix.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            // Row 0
            Self::N1 => (0, 0),
            Self::N2 => (0, 1),
            Self::N3 => (0, 2),
            Self::N4 => (0, 3),
            Self::N5 => (0, 4),
            Self::N6 => (0, 5),
            Self::N7 => (0, 6),
            Self::N8 => (0, 7),
            // Row 1
            Self::N9 => (1, 0),
            Self::N0 => (1, 1),
            Self::Minus => (1, 2),
            Self::Equal => (1, 3),
            Self::Backslash => (1, 4),
            Self::BracketLeft => (1, 5),
            Self::BracketRight => (1, 6),
            Self::Semicolon => (1, 7),
            // Row 2
            Self::Quote => (2, 0),
            Self::Comma => (2, 1),
            Self::Period => (2, 2),
            Self::Slash => (2, 3),
            Self::Dead => (2, 4),
            Self::Pound => (2, 5),
            Self::Delete => (2, 6),
            Self::CtrlLeft => (2, 7),
            // Row 3
            Self::Shift => (3, 0),
            Self::Z => (3, 1),
            Self::X => (3, 2),
            Self::C => (3, 3),
            Self::V => (3, 4),
            Self::B => (3, 5),
            Self::N => (3, 6),
            Self::M => (3, 7),
            // Row 4
            Self::A => (4, 0),
            Self::S => (4, 1),
            Self::D => (4, 2),
            Self::F => (4, 3),
            Self::G => (4, 4),
            Self::H => (4, 5),
            Self::J => (4, 6),
            Self::K => (4, 7),
            // Row 5
            Self::L => (5, 0),
            Self::Enter => (5, 1),
            Self::Q => (5, 2),
            Self::W => (5, 3),
            Self::E => (5, 4),
            Self::R => (5, 5),
            Self::T => (5, 6),
            Self::Y => (5, 7),
            // Row 6
            Self::U => (6, 0),
            Self::I => (6, 1),
            Self::O => (6, 2),
            Self::P => (6, 3),
            Self::Escape => (6, 4),
            Self::Tab => (6, 5),
            Self::CapsLock => (6, 6),
            Self::Space => (6, 7),
            // Row 7
            Self::F1 => (7, 0),
            Self::F2 => (7, 1),
            Self::F3 => (7, 2),
            Self::F4 => (7, 3),
            Self::F5 => (7, 4),
            Self::Left => (7, 5),
            Self::Right => (7, 6),
            Self::Up => (7, 7),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_bounds() {
        assert_eq!(MtxKey::N1.matrix(), (0, 0));
        assert_eq!(MtxKey::N8.matrix(), (0, 7));
        assert_eq!(MtxKey::Up.matrix(), (7, 7));
        assert_eq!(MtxKey::Space.matrix(), (6, 7));
    }
}
