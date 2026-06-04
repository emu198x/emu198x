//! VIC-20 keyboard key enum and 8 × 8 matrix.
//!
//! The keyboard is an 8 × 8 matrix scanned through VIA #2 ($9120): the
//! KERNAL drives a column-select pattern on port B (PB, all outputs) and
//! reads the row state on port A (PA, all inputs). Each key shorts one PA
//! row line to one PB column line.
//!
//! [`Vic20Key::matrix`] returns `(row, col)` where `row` is the PA read
//! line and `col` is the PB drive line — matching the emulator's
//! [`crate::KeyboardState`] electrical model. The table is transcribed
//! from the Minimig/MiSTer VIC-20 core's keyboard matrix
//! (`fpga64_keyboard.vhd`), cross-checked against VICE's positional
//! keymap.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vic20Key {
    // Letters
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
    // Digits
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
    // Punctuation / symbols (unshifted legends)
    Plus,
    Minus,
    Asterisk,
    Slash,
    Equal,
    Colon,
    Semicolon,
    Comma,
    Period,
    At,
    Pound,
    ArrowUp,
    ArrowLeft,
    // Control / editing
    Space,
    Return,
    Delete,
    Home,
    Stop,
    Ctrl,
    Commodore,
    ShiftLeft,
    ShiftRight,
    CursorRight,
    CursorDown,
    // Function keys
    F1,
    F3,
    F5,
    F7,
}

impl Vic20Key {
    /// Return the `(row, col)` pair in the 8 × 8 matrix, where `row` is
    /// the VIA #2 PA read line and `col` is the PB column-drive line.
    ///
    /// Transcribed cell-by-cell from the real KERNAL: each position was
    /// probed against the boot ROM's keyboard decode and the produced
    /// PETSCII / cursor behaviour recorded, so the table reflects exactly
    /// what the hardware ROM expects rather than any third-party labelling.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            // Row 0 (PA0)
            Self::Num1 => (0, 0),
            Self::ArrowLeft => (0, 1),
            Self::Ctrl => (0, 2),
            Self::Stop => (0, 3),
            Self::Space => (0, 4),
            Self::Commodore => (0, 5),
            Self::Q => (0, 6),
            Self::Num2 => (0, 7),
            // Row 1 (PA1)
            Self::Num3 => (1, 0),
            Self::W => (1, 1),
            Self::A => (1, 2),
            Self::ShiftLeft => (1, 3),
            Self::Z => (1, 4),
            Self::S => (1, 5),
            Self::E => (1, 6),
            Self::Num4 => (1, 7),
            // Row 2 (PA2)
            Self::Num5 => (2, 0),
            Self::R => (2, 1),
            Self::D => (2, 2),
            Self::X => (2, 3),
            Self::C => (2, 4),
            Self::F => (2, 5),
            Self::T => (2, 6),
            Self::Num6 => (2, 7),
            // Row 3 (PA3)
            Self::Num7 => (3, 0),
            Self::Y => (3, 1),
            Self::G => (3, 2),
            Self::V => (3, 3),
            Self::B => (3, 4),
            Self::H => (3, 5),
            Self::U => (3, 6),
            Self::Num8 => (3, 7),
            // Row 4 (PA4)
            Self::Num9 => (4, 0),
            Self::I => (4, 1),
            Self::J => (4, 2),
            Self::N => (4, 3),
            Self::M => (4, 4),
            Self::K => (4, 5),
            Self::O => (4, 6),
            Self::Num0 => (4, 7),
            // Row 5 (PA5)
            Self::Plus => (5, 0),
            Self::P => (5, 1),
            Self::L => (5, 2),
            Self::Comma => (5, 3),
            Self::Period => (5, 4),
            Self::Colon => (5, 5),
            Self::At => (5, 6),
            Self::Minus => (5, 7),
            // Row 6 (PA6)
            Self::Pound => (6, 0),
            Self::Asterisk => (6, 1),
            Self::Semicolon => (6, 2),
            Self::Slash => (6, 3),
            Self::ShiftRight => (6, 4),
            Self::Equal => (6, 5),
            Self::ArrowUp => (6, 6),
            Self::Home => (6, 7),
            // Row 7 (PA7)
            Self::Delete => (7, 0),
            Self::Return => (7, 1),
            Self::CursorRight => (7, 2),
            Self::CursorDown => (7, 3),
            Self::F1 => (7, 4),
            Self::F3 => (7, 5),
            Self::F5 => (7, 6),
            Self::F7 => (7, 7),
        }
    }
}
