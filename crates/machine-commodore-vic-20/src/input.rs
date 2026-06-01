//! VIC-20 keyboard key enum (8 × 8 matrix scanned through VIA 6522s
//! on real hardware — left stubbed here pending VIA wiring).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vic20Key {
    Return,
    Space,
    Stop,
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
}

impl Vic20Key {
    /// Return the (row, column) pair in the 8 × 8 matrix. Placeholder
    /// mapping until VIA-driven keyboard scan is wired.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        // Rough Commodore-VIC-20-style mapping; see donor's input.rs for the
        // complete table once VIAs are wired.
        match self {
            Self::Return => (0, 1),
            Self::Space => (0, 4),
            Self::Stop => (7, 7),
            Self::A => (1, 2),
            Self::B => (3, 4),
            Self::C => (2, 4),
            Self::D => (2, 2),
            Self::E => (1, 6),
            Self::F => (2, 5),
            Self::G => (3, 2),
            Self::H => (3, 5),
            Self::I => (4, 1),
            Self::J => (4, 2),
            Self::K => (4, 5),
            Self::L => (5, 2),
            Self::M => (4, 4),
            Self::N => (4, 7),
            Self::O => (4, 6),
            Self::P => (5, 1),
            Self::Q => (0, 6),
            Self::R => (2, 1),
            Self::S => (1, 5),
            Self::T => (2, 6),
            Self::U => (3, 6),
            Self::V => (3, 7),
            Self::W => (1, 1),
            Self::X => (2, 7),
            Self::Y => (3, 1),
            Self::Z => (1, 4),
        }
    }
}
