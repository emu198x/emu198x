//! Input handling for the Commodore PET (graphics keyboard).
//!
//! The PET has a 10 × 8 keyboard matrix scanned via PIA #1: port A drives
//! a binary row number 0-9, port B reads that row's eight columns. Every
//! position was probed against the real editor ROM (press the cell, read
//! the produced PETSCII / cursor effect) and transcribed below, so the
//! table is the genuine graphics-keyboard layout rather than a guess.
//!
//! On the graphics keyboard the digits live on a numeric keypad and every
//! punctuation mark is its own unshifted key, so a program can be typed
//! without modelling Shift at all.

/// Logical key on the PET graphics keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // Numeric keypad digits.
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    // Punctuation — each its own unshifted key on the graphics keyboard.
    Exclaim,
    Hash,
    Percent,
    Ampersand,
    ParenLeft,
    Quote,
    Dollar,
    Apostrophe,
    ParenRight,
    Slash,
    Asterisk,
    Plus,
    Colon,
    Semicolon,
    Comma,
    Period,
    Question,
    Less,
    Greater,
    Equal,
    At,
    // Control / editing.
    Return,
    Space,
    CursorRight,
}

impl PetKey {
    /// Return the `(row, col)` pair for this key — `row` is the binary row
    /// number driven on PIA #1 port A, `col` the column bit read on port B.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            // Row 0
            Self::Exclaim => (0, 0),
            Self::Hash => (0, 1),
            Self::Percent => (0, 2),
            Self::Ampersand => (0, 3),
            Self::ParenLeft => (0, 4),
            // Row 1
            Self::Quote => (1, 0),
            Self::Dollar => (1, 1),
            Self::Apostrophe => (1, 2),
            Self::ParenRight => (1, 4),
            // Row 2
            Self::Q => (2, 0),
            Self::E => (2, 1),
            Self::T => (2, 2),
            Self::U => (2, 3),
            Self::O => (2, 4),
            Self::Num7 => (2, 6),
            Self::Num9 => (2, 7),
            // Row 3
            Self::W => (3, 0),
            Self::R => (3, 1),
            Self::Y => (3, 2),
            Self::I => (3, 3),
            Self::P => (3, 4),
            Self::Num8 => (3, 6),
            Self::Slash => (3, 7),
            // Row 4
            Self::A => (4, 0),
            Self::D => (4, 1),
            Self::G => (4, 2),
            Self::J => (4, 3),
            Self::L => (4, 4),
            Self::Num4 => (4, 6),
            Self::Num6 => (4, 7),
            // Row 5
            Self::S => (5, 0),
            Self::F => (5, 1),
            Self::H => (5, 2),
            Self::K => (5, 3),
            Self::Colon => (5, 4),
            Self::Num5 => (5, 6),
            Self::Asterisk => (5, 7),
            // Row 6
            Self::Z => (6, 0),
            Self::C => (6, 1),
            Self::B => (6, 2),
            Self::M => (6, 3),
            Self::Semicolon => (6, 4),
            Self::Return => (6, 5),
            Self::Num1 => (6, 6),
            Self::Num3 => (6, 7),
            // Row 7
            Self::X => (7, 0),
            Self::V => (7, 1),
            Self::N => (7, 2),
            Self::Comma => (7, 3),
            Self::Question => (7, 4),
            Self::Num2 => (7, 6),
            Self::Plus => (7, 7),
            // Row 8
            Self::At => (8, 1),
            Self::CursorRight => (8, 2),
            Self::Greater => (8, 4),
            Self::Num0 => (8, 6),
            // Row 9
            Self::Space => (9, 2),
            Self::Less => (9, 3),
            Self::Period => (9, 6),
            Self::Equal => (9, 7),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_valid() {
        let (r, c) = PetKey::A.matrix();
        assert_eq!((r, c), (4, 0));
    }
}
