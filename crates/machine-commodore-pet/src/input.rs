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
//!
//! The table is complete: every cell the keyboard drives is here, and the
//! cells MAME's `pet` driver marks unused — (1,5), (3,5), (4,5), (5,5),
//! (7,5), (8,3), (9,5) — are the only gaps. Completeness is the point. The
//! table was previously partial, and a missing key is invisible: it does
//! not fail, it just cannot be pressed, so `type_string` refuses a
//! character with no hint that the matrix is where the hole is.

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
    Minus,
    At,
    BracketLeft,
    BracketRight,
    Backslash,
    /// The `↑` keycap, left of the keypad. Types `^` — BASIC's power
    /// operator — and carries π on its shifted legend.
    UpArrow,
    /// The `←` keycap, top left. PETSCII $5F; BASIC reads it as the
    /// token delimiter, and it is not any ASCII character.
    LeftArrow,
    // Control / editing.
    Return,
    Space,
    Home,
    /// Cursor right; shifted, cursor left. One keycap, both directions.
    CursorRight,
    /// Cursor down; shifted, cursor up. One keycap, both directions.
    CursorDown,
    /// Delete; shifted, insert.
    Delete,
    ShiftLeft,
    ShiftRight,
    RvsOff,
    /// Stop; shifted, Run.
    ///
    /// The cell is right — holding it moves the row-9 column read at $9B
    /// from `255` to `239`, bit 4 clear — but a running BASIC program does
    /// not break. That is a fault somewhere past the scan, tracked as
    /// #1212; pressing this key registers and does nothing visible.
    StopRun,
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
            Self::LeftArrow => (0, 5),
            Self::Home => (0, 6),
            Self::CursorRight => (0, 7),
            // Row 1
            Self::Quote => (1, 0),
            Self::Dollar => (1, 1),
            Self::Apostrophe => (1, 2),
            Self::Backslash => (1, 3),
            Self::ParenRight => (1, 4),
            Self::CursorDown => (1, 6),
            Self::Delete => (1, 7),
            // Row 2
            Self::Q => (2, 0),
            Self::E => (2, 1),
            Self::T => (2, 2),
            Self::U => (2, 3),
            Self::O => (2, 4),
            Self::UpArrow => (2, 5),
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
            Self::ShiftLeft => (8, 0),
            Self::At => (8, 1),
            Self::BracketRight => (8, 2),
            Self::Greater => (8, 4),
            Self::ShiftRight => (8, 5),
            Self::Num0 => (8, 6),
            Self::Minus => (8, 7),
            // Row 9
            Self::RvsOff => (9, 0),
            Self::BracketLeft => (9, 1),
            Self::Space => (9, 2),
            Self::Less => (9, 3),
            Self::StopRun => (9, 4),
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

    /// Every key this machine has, so the collision check below sees the
    /// whole matrix rather than whichever keys someone remembered.
    const ALL_KEYS: &[PetKey] = &[
        PetKey::A,
        PetKey::B,
        PetKey::C,
        PetKey::D,
        PetKey::E,
        PetKey::F,
        PetKey::G,
        PetKey::H,
        PetKey::I,
        PetKey::J,
        PetKey::K,
        PetKey::L,
        PetKey::M,
        PetKey::N,
        PetKey::O,
        PetKey::P,
        PetKey::Q,
        PetKey::R,
        PetKey::S,
        PetKey::T,
        PetKey::U,
        PetKey::V,
        PetKey::W,
        PetKey::X,
        PetKey::Y,
        PetKey::Z,
        PetKey::Num0,
        PetKey::Num1,
        PetKey::Num2,
        PetKey::Num3,
        PetKey::Num4,
        PetKey::Num5,
        PetKey::Num6,
        PetKey::Num7,
        PetKey::Num8,
        PetKey::Num9,
        PetKey::Exclaim,
        PetKey::Hash,
        PetKey::Percent,
        PetKey::Ampersand,
        PetKey::ParenLeft,
        PetKey::Quote,
        PetKey::Dollar,
        PetKey::Apostrophe,
        PetKey::ParenRight,
        PetKey::Slash,
        PetKey::Asterisk,
        PetKey::Plus,
        PetKey::Colon,
        PetKey::Semicolon,
        PetKey::Comma,
        PetKey::Period,
        PetKey::Question,
        PetKey::Less,
        PetKey::Greater,
        PetKey::Equal,
        PetKey::Minus,
        PetKey::At,
        PetKey::Return,
        PetKey::Space,
        PetKey::BracketLeft,
        PetKey::BracketRight,
        PetKey::Backslash,
        PetKey::UpArrow,
        PetKey::LeftArrow,
        PetKey::Home,
        PetKey::CursorRight,
        PetKey::CursorDown,
        PetKey::Delete,
        PetKey::ShiftLeft,
        PetKey::ShiftRight,
        PetKey::RvsOff,
        PetKey::StopRun,
    ];

    /// Two keys sharing a cell is silent: the later one simply presses the
    /// earlier one, and the machine types the wrong character with no error
    /// anywhere. Adding a key means picking a free cell, so check it is free.
    #[test]
    fn no_two_keys_share_a_matrix_cell() {
        let mut seen: Vec<((usize, u8), PetKey)> = Vec::new();
        for &key in ALL_KEYS {
            let cell = key.matrix();
            assert!(cell.0 < 10 && cell.1 < 8, "{key:?} is outside the matrix");
            if let Some((_, other)) = seen.iter().find(|(c, _)| *c == cell) {
                panic!("{key:?} and {other:?} both claim cell {cell:?}");
            }
            seen.push((cell, key));
        }
    }

    /// The keypad's bottom row is `0 . - =`, and its two columns are 6 and 7
    /// throughout, so minus sits beside `=` at row 8 column 7 — the cell MAME
    /// gives it as `ROW8` bit `0x80`.
    ///
    /// `(8, 2)` is in here too because it used to be labelled `CursorRight`,
    /// which is a different key at `(0, 7)`. Pressing it deposited `]` on
    /// screen rather than moving the cursor — the mapping did something, so
    /// nothing looked broken.
    #[test]
    fn keypad_minus_sits_beside_equals() {
        assert_eq!(PetKey::Minus.matrix(), (8, 7));
        assert_eq!(PetKey::Equal.matrix(), (9, 7));
        assert_eq!(PetKey::Num0.matrix(), (8, 6));
        assert_eq!(PetKey::Period.matrix(), (9, 6));
        assert_eq!(PetKey::BracketRight.matrix(), (8, 2));
        assert_eq!(PetKey::CursorRight.matrix(), (0, 7));
    }

    /// The matrix is complete, so the only empty cells are the seven the
    /// keyboard genuinely does not drive. Pin them: a key added to a cell
    /// that is not on this list means either the key or the list is wrong.
    #[test]
    fn only_the_undriven_cells_are_empty() {
        const UNDRIVEN: &[(usize, u8)] = &[(1, 5), (3, 5), (4, 5), (5, 5), (7, 5), (8, 3), (9, 5)];
        let mut filled = [[false; 8]; 10];
        for &key in ALL_KEYS {
            let (r, c) = key.matrix();
            filled[r][usize::from(c)] = true;
        }
        for (r, row) in filled.iter().enumerate() {
            for c in 0..8u8 {
                let expected = !UNDRIVEN.contains(&(r, c));
                let claimed = row[usize::from(c)];
                assert_eq!(
                    claimed,
                    expected,
                    "cell ({r}, {c}) is {} but should be {}",
                    if claimed { "claimed" } else { "empty" },
                    if expected { "claimed" } else { "empty" },
                );
            }
        }
    }
}
