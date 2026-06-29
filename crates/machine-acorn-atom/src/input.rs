//! Input handling for the Acorn Atom.
//!
//! The Atom keyboard is an 8255-scanned matrix: port A drives a binary
//! 0-9 index into a 4-to-10 line decoder (the manual's "rows"), and the
//! six column lines come back on port B. [`AtomKey::matrix`] returns
//! `(row, col)` where `row` is that decoder index and `col` the port B
//! bit. Every printable cell was probed against the real MOS ROM and
//! transcribed below.
//!
//! SHIFT and CTRL are not part of the scanned 6×10 matrix: they are read on
//! port B bits 7 and 6 respectively, common to every column, active-low
//! (Atom Technical Manual / *Atomic Theory and Practice* §25.5 — port B bit 6 =
//! CTRL, bit 7 = SHIFT, "low when pressed"; cross-checked against Atomulator
//! `8255.c` `read8255` case 1). [`AtomKey::Shift`] / [`AtomKey::Ctrl`] are
//! handled separately from [`AtomKey::matrix`], which only covers the scanned
//! keys. The Atom places its shifted symbols on the digit keys like a
//! typewriter — e.g. `"` is SHIFT+2 — so SHIFT plus an existing key is enough.

/// Logical key on the Acorn Atom keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomKey {
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
    Comma,
    Semicolon,
    Colon,
    Period,
    Slash,
    At,
    Return,
    Space,
    /// SHIFT — read on port B bit 7 (active-low), not in the scanned matrix.
    Shift,
    /// CTRL — read on port B bit 6 (active-low), not in the scanned matrix.
    Ctrl,
}

impl AtomKey {
    /// Returns `true` for the modifier keys (SHIFT / CTRL), which are read on
    /// port B bits 7 / 6 rather than the scanned matrix.
    #[must_use]
    pub const fn is_modifier(self) -> bool {
        matches!(self, Self::Shift | Self::Ctrl)
    }

    /// Return the `(row, col)` matrix position — `row` is the binary
    /// decoder index driven on 8255 port A, `col` the port B column bit.
    ///
    /// Returns `None` for the modifier keys ([`Self::Shift`] / [`Self::Ctrl`]),
    /// which are not part of the scanned matrix.
    #[must_use]
    pub const fn matrix(self) -> Option<(usize, usize)> {
        let position = match self {
            // col 1 — digits 3 2 1 0
            Self::Num3 => (0, 1),
            Self::Num2 => (1, 1),
            Self::Num1 => (2, 1),
            Self::Num0 => (3, 1),
            // col 2 — punctuation then digits 9..4
            Self::Comma => (1, 2),
            Self::Semicolon => (2, 2),
            Self::Colon => (3, 2),
            Self::Num9 => (4, 2),
            Self::Num8 => (5, 2),
            Self::Num7 => (6, 2),
            Self::Num6 => (7, 2),
            Self::Num5 => (8, 2),
            Self::Num4 => (9, 2),
            // col 3 — G F E D C B A @ / .
            Self::G => (0, 3),
            Self::F => (1, 3),
            Self::E => (2, 3),
            Self::D => (3, 3),
            Self::C => (4, 3),
            Self::B => (5, 3),
            Self::A => (6, 3),
            Self::At => (7, 3),
            Self::Slash => (8, 3),
            Self::Period => (9, 3),
            // col 4 — Q P O N M L K J I H
            Self::Q => (0, 4),
            Self::P => (1, 4),
            Self::O => (2, 4),
            Self::N => (3, 4),
            Self::M => (4, 4),
            Self::L => (5, 4),
            Self::K => (6, 4),
            Self::J => (7, 4),
            Self::I => (8, 4),
            Self::H => (9, 4),
            // col 5 — Z Y X W V U T S R
            Self::Z => (1, 5),
            Self::Y => (2, 5),
            Self::X => (3, 5),
            Self::W => (4, 5),
            Self::V => (5, 5),
            Self::U => (6, 5),
            Self::T => (7, 5),
            Self::S => (8, 5),
            Self::R => (9, 5),
            // control / editing
            Self::Return => (6, 1),
            Self::Space => (0, 0),
            // Modifiers are read on port B bits 6/7, not the scanned matrix.
            Self::Shift | Self::Ctrl => return None,
        };
        Some(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_valid() {
        assert_eq!(AtomKey::A.matrix(), Some((6, 3)));
        assert_eq!(AtomKey::Return.matrix(), Some((6, 1)));
    }

    #[test]
    fn modifiers_have_no_matrix_position() {
        assert!(AtomKey::Shift.is_modifier());
        assert!(AtomKey::Ctrl.is_modifier());
        assert_eq!(AtomKey::Shift.matrix(), None);
        assert_eq!(AtomKey::Ctrl.matrix(), None);
        assert!(!AtomKey::A.is_modifier());
    }
}
