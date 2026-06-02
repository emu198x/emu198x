//! Input handling for the Commodore PET.
//!
//! The PET has a 10x8 keyboard matrix scanned via the PIA/VIA.

/// Logical key on the PET keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetKey {
    // Row 0
    N2,
    N5,
    N8,
    Minus,
    N0NumPad,
    DotNumPad,
    // Row 1
    N1,
    N4,
    N7,
    N0,
    N3NumPad,
    N6NumPad,
    N9NumPad,
    // Row 2
    Escape,
    S,
    F,
    H,
    BracketClose,
    Return,
    // Row 3
    A,
    D,
    G,
    J,
    SemiColon,
    CursorLeft,
    // Row 4
    Tab,
    W,
    R,
    Y,
    BackSlash,
    Del,
    // Row 5
    Q,
    E,
    T,
    U,
    BracketOpen,
    CursorDown,
    // Row 6
    LeftShift,
    C,
    B,
    Period,
    N8NumPad,
    // Row 7
    Z,
    V,
    N,
    Comma,
    N7NumPad,
    CursorUp,
    // Row 8
    RightShift,
    X,
    M,
    Slash,
    N4NumPad,
    N1NumPad,
    // Row 9
    RvsOff,
    Space,
    K,
    Colon,
    N5NumPad,
    N2NumPad,

    // Extra
    N3,
    N6,
    N9,
    Plus,
    Equals,
    Home,
    RunStop,
    L,
    O,
    I,
    P,
    At,
}

impl PetKey {
    /// Return the (row, column) pair for this key.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            Self::N2 => (0, 0),
            Self::N5 => (0, 1),
            Self::N8 => (0, 2),
            Self::Minus => (0, 3),
            Self::N0NumPad => (0, 4),
            Self::DotNumPad => (0, 5),

            Self::N1 => (1, 0),
            Self::N4 => (1, 1),
            Self::N7 => (1, 2),
            Self::N0 => (1, 3),
            Self::N3NumPad => (1, 4),
            Self::N6NumPad => (1, 5),
            Self::N9NumPad => (1, 6),

            Self::Escape => (2, 0),
            Self::S => (2, 1),
            Self::F => (2, 2),
            Self::H => (2, 3),
            Self::BracketClose => (2, 4),
            Self::Return => (2, 5),

            Self::A => (3, 0),
            Self::D => (3, 1),
            Self::G => (3, 2),
            Self::J => (3, 3),
            Self::SemiColon => (3, 4),
            Self::CursorLeft => (3, 5),

            Self::Tab => (4, 0),
            Self::W => (4, 1),
            Self::R => (4, 2),
            Self::Y => (4, 3),
            Self::BackSlash => (4, 4),
            Self::Del => (4, 5),

            Self::Q => (5, 0),
            Self::E => (5, 1),
            Self::T => (5, 2),
            Self::U => (5, 3),
            Self::BracketOpen => (5, 4),
            Self::CursorDown => (5, 5),

            Self::LeftShift => (6, 0),
            Self::C => (6, 1),
            Self::B => (6, 2),
            Self::Period => (6, 3),
            Self::N8NumPad => (6, 4),

            Self::Z => (7, 0),
            Self::V => (7, 1),
            Self::N => (7, 2),
            Self::Comma => (7, 3),
            Self::N7NumPad => (7, 4),
            Self::CursorUp => (7, 5),

            Self::RightShift => (8, 0),
            Self::X => (8, 1),
            Self::M => (8, 2),
            Self::Slash => (8, 3),
            Self::N4NumPad => (8, 4),
            Self::N1NumPad => (8, 5),

            Self::RvsOff => (9, 0),
            Self::Space => (9, 1),
            Self::K => (9, 2),
            Self::Colon => (9, 3),
            Self::N5NumPad => (9, 4),
            Self::N2NumPad => (9, 5),

            Self::N3 => (0, 6),
            Self::N6 => (0, 7),
            Self::N9 => (1, 7),
            Self::Plus => (2, 6),
            Self::Equals => (2, 7),
            Self::Home => (3, 6),
            Self::RunStop => (3, 7),
            Self::L => (4, 6),
            Self::O => (4, 7),
            Self::I => (5, 6),
            Self::P => (5, 7),
            Self::At => (6, 5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_valid() {
        let (r, c) = PetKey::A.matrix();
        assert!(r < 10);
        assert!(c < 8);
    }
}
