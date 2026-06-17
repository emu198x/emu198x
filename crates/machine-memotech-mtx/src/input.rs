//! Input handling for the Memotech MTX.
//!
//! The keyboard is a drive/sense matrix: the CPU writes a **drive** byte to
//! port `$05` (each zero bit drives one of eight columns, active low) and reads
//! the sense lines back — the low eight on `$05`, the two extra on `$06`. A
//! pressed key pulls its sense bit low. Every position below is the real MTX
//! wiring taken from MAME's `memotech/mtx.cpp` (`ROWn` = drive column n, each
//! `PORT_BIT` mask = the sense line). The earlier table was a donor placeholder
//! whose key positions did not match the hardware — typing `ABCDE` echoed
//! `@uf11` because the cells were wrong.

/// Logical key on the MTX keyboard.
///
/// Each key maps to a `(column, sense-bit)` pair in the matrix via
/// [`MtxKey::matrix`]. Columns are the eight drive lines; sense bits 0-7 read
/// back on `$05`, bits 8-9 on `$06`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MtxKey {
    // Letters.
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
    // Digits.
    N0,
    N1,
    N2,
    N3,
    N4,
    N5,
    N6,
    N7,
    N8,
    N9,
    // Punctuation (unshifted legends).
    Minus,
    Backslash,
    Caret,
    At,
    Semicolon,
    Colon,
    BracketLeft,
    BracketRight,
    Comma,
    Period,
    Slash,
    Underscore,
    // Modifiers and editing.
    Escape,
    Ctrl,
    ShiftLeft,
    ShiftRight,
    CapsLock,
    Enter,
    Space,
    LineFeed,
    Backspace,
    Tab,
    Delete,
    // Cursor keys (numeric-keypad legends 5/1/3/.).
    Up,
    Down,
    Left,
    Right,
    // Remaining numeric-keypad keys.
    Home,
    Insert,
    Page,
    Break,
    EndOfLine,
    KeypadEnter,
    // Function keys.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
}

impl MtxKey {
    /// Return the `(column, sense-bit)` pair for this key in the matrix.
    ///
    /// Sourced from MAME `memotech/mtx.cpp`: the `ROWn` port is drive column
    /// `n`, and each `PORT_BIT` mask (`0x001`..=`0x200`) is the sense bit
    /// (`0x080` = bit 7 on `$05`, `0x100`/`0x200` = bits 8/9 on `$06`).
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            // Column 0.
            Self::N1 => (0, 0),
            Self::N3 => (0, 1),
            Self::N5 => (0, 2),
            Self::N7 => (0, 3),
            Self::N9 => (0, 4),
            Self::Minus => (0, 5),
            Self::Backslash => (0, 6),
            Self::Page => (0, 7),
            Self::Break => (0, 8),
            Self::F1 => (0, 9),
            // Column 1.
            Self::Escape => (1, 0),
            Self::N2 => (1, 1),
            Self::N4 => (1, 2),
            Self::N6 => (1, 3),
            Self::N8 => (1, 4),
            Self::N0 => (1, 5),
            Self::Caret => (1, 6),
            Self::EndOfLine => (1, 7),
            Self::Backspace => (1, 8),
            Self::F5 => (1, 9),
            // Column 2.
            Self::Ctrl => (2, 0),
            Self::W => (2, 1),
            Self::R => (2, 2),
            Self::Y => (2, 3),
            Self::I => (2, 4),
            Self::P => (2, 5),
            Self::BracketLeft => (2, 6),
            Self::Up => (2, 7),
            Self::Tab => (2, 8),
            Self::F2 => (2, 9),
            // Column 3.
            Self::Q => (3, 0),
            Self::E => (3, 1),
            Self::T => (3, 2),
            Self::U => (3, 3),
            Self::O => (3, 4),
            Self::At => (3, 5),
            Self::LineFeed => (3, 6),
            Self::Left => (3, 7),
            Self::Delete => (3, 8),
            Self::F6 => (3, 9),
            // Column 4.
            Self::CapsLock => (4, 0),
            Self::S => (4, 1),
            Self::F => (4, 2),
            Self::H => (4, 3),
            Self::K => (4, 4),
            Self::Semicolon => (4, 5),
            Self::BracketRight => (4, 6),
            Self::Right => (4, 7),
            Self::F7 => (4, 9),
            // Column 5.
            Self::A => (5, 0),
            Self::D => (5, 1),
            Self::G => (5, 2),
            Self::J => (5, 3),
            Self::L => (5, 4),
            Self::Colon => (5, 5),
            Self::Enter => (5, 6),
            Self::Home => (5, 7),
            Self::F3 => (5, 9),
            // Column 6.
            Self::ShiftLeft => (6, 0),
            Self::X => (6, 1),
            Self::V => (6, 2),
            Self::N => (6, 3),
            Self::Comma => (6, 4),
            Self::Slash => (6, 5),
            Self::ShiftRight => (6, 6),
            Self::Down => (6, 7),
            Self::F8 => (6, 9),
            // Column 7.
            Self::Z => (7, 0),
            Self::C => (7, 1),
            Self::B => (7, 2),
            Self::M => (7, 3),
            Self::Period => (7, 4),
            Self::Underscore => (7, 5),
            Self::Insert => (7, 6),
            Self::KeypadEnter => (7, 7),
            Self::Space => (7, 8),
            Self::F4 => (7, 9),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keys_sit_on_sense_bit_7_of_their_columns() {
        // Numeric-keypad cursor legends, per MAME: Up=Kp5 (col2), Left=Kp1
        // (col3), Right=Kp3 (col4), Down=Kp. (col6), all on sense bit 7.
        assert_eq!(MtxKey::Up.matrix(), (2, 7));
        assert_eq!(MtxKey::Left.matrix(), (3, 7));
        assert_eq!(MtxKey::Right.matrix(), (4, 7));
        assert_eq!(MtxKey::Down.matrix(), (6, 7));
    }

    #[test]
    fn home_row_and_space_match_the_hardware_matrix() {
        assert_eq!(MtxKey::A.matrix(), (5, 0));
        assert_eq!(MtxKey::N1.matrix(), (0, 0));
        assert_eq!(MtxKey::N2.matrix(), (1, 1));
        assert_eq!(MtxKey::Space.matrix(), (7, 8));
        assert_eq!(MtxKey::Enter.matrix(), (5, 6));
    }
}
