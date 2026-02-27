//! Input handling for the C64.
//!
//! Three layers:
//! 1. `C64Key` — logical key names mapped to the 8x8 keyboard matrix.
//! 2. Immediate `press_key`/`release_key` methods on `C64`.
//! 3. `InputQueue` — timed key events for scripted sequences.

use std::collections::VecDeque;

use crate::keyboard::KeyboardMatrix;

/// Logical key on the C64 keyboard.
///
/// Each key maps to a (row, col) pair in the 8x8 keyboard matrix.
///
/// Matrix layout (row = CIA1 PA, col = CIA1 PB):
///
/// | Row | Col0 | Col1 | Col2 | Col3 | Col4 | Col5 | Col6 | Col7    |
/// |-----|------|------|------|------|------|------|------|---------|
/// | 0   | DEL  | 3    | 5    | 7    | 9    | +    | £    | 1       |
/// | 1   | RET  | W    | R    | Y    | I    | P    | *    | ←       |
/// | 2   | →    | A    | D    | G    | J    | L    | ;    | CTRL    |
/// | 3   | F7   | 4    | 6    | 8    | 0    | -    | HOME | 2       |
/// | 4   | F1   | Z    | C    | B    | M    | .    | RSHFT| SPC     |
/// | 5   | F3   | S    | F    | H    | K    | :    | =    | C=      |
/// | 6   | F5   | E    | T    | U    | O    | @    | ↑    | Q       |
/// | 7   | ↓    | LSHFT| X    | V    | N    | ,    | /    | STOP    |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum C64Key {
    // Row 0
    Delete,
    N3,
    N5,
    N7,
    N9,
    Plus,
    Pound,
    N1,
    // Row 1
    Return,
    W,
    R,
    Y,
    I,
    P,
    Asterisk,
    LeftArrow,
    // Row 2
    CursorRight,
    A,
    D,
    G,
    J,
    L,
    Semicolon,
    Ctrl,
    // Row 3
    F7,
    N4,
    N6,
    N8,
    N0,
    Minus,
    Home,
    N2,
    // Row 4
    F1,
    Z,
    C,
    B,
    M,
    Period,
    RShift,
    Space,
    // Row 5
    F3,
    S,
    F,
    H,
    K,
    Colon,
    Equals,
    Commodore,
    // Row 6
    F5,
    E,
    T,
    U,
    O,
    At,
    UpArrow,
    Q,
    // Row 7
    CursorDown,
    LShift,
    X,
    V,
    N,
    Comma,
    Slash,
    RunStop,
}

impl C64Key {
    /// Return the (row, col) pair for this key in the keyboard matrix.
    #[must_use]
    pub const fn matrix(self) -> (u8, u8) {
        match self {
            // Row 0
            Self::Delete => (0, 0),
            Self::N3 => (0, 1),
            Self::N5 => (0, 2),
            Self::N7 => (0, 3),
            Self::N9 => (0, 4),
            Self::Plus => (0, 5),
            Self::Pound => (0, 6),
            Self::N1 => (0, 7),
            // Row 1
            Self::Return => (1, 0),
            Self::W => (1, 1),
            Self::R => (1, 2),
            Self::Y => (1, 3),
            Self::I => (1, 4),
            Self::P => (1, 5),
            Self::Asterisk => (1, 6),
            Self::LeftArrow => (1, 7),
            // Row 2
            Self::CursorRight => (2, 0),
            Self::A => (2, 1),
            Self::D => (2, 2),
            Self::G => (2, 3),
            Self::J => (2, 4),
            Self::L => (2, 5),
            Self::Semicolon => (2, 6),
            Self::Ctrl => (2, 7),
            // Row 3
            Self::F7 => (3, 0),
            Self::N4 => (3, 1),
            Self::N6 => (3, 2),
            Self::N8 => (3, 3),
            Self::N0 => (3, 4),
            Self::Minus => (3, 5),
            Self::Home => (3, 6),
            Self::N2 => (3, 7),
            // Row 4
            Self::F1 => (4, 0),
            Self::Z => (4, 1),
            Self::C => (4, 2),
            Self::B => (4, 3),
            Self::M => (4, 4),
            Self::Period => (4, 5),
            Self::RShift => (4, 6),
            Self::Space => (4, 7),
            // Row 5
            Self::F3 => (5, 0),
            Self::S => (5, 1),
            Self::F => (5, 2),
            Self::H => (5, 3),
            Self::K => (5, 4),
            Self::Colon => (5, 5),
            Self::Equals => (5, 6),
            Self::Commodore => (5, 7),
            // Row 6
            Self::F5 => (6, 0),
            Self::E => (6, 1),
            Self::T => (6, 2),
            Self::U => (6, 3),
            Self::O => (6, 4),
            Self::At => (6, 5),
            Self::UpArrow => (6, 6),
            Self::Q => (6, 7),
            // Row 7
            Self::CursorDown => (7, 0),
            Self::LShift => (7, 1),
            Self::X => (7, 2),
            Self::V => (7, 3),
            Self::N => (7, 4),
            Self::Comma => (7, 5),
            Self::Slash => (7, 6),
            Self::RunStop => (7, 7),
        }
    }
}

/// A timed keyboard event.
#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Frame number at which this event fires.
    pub frame: u64,
    /// Which key.
    pub key: C64Key,
    /// True = press, false = release.
    pub pressed: bool,
}

/// Timed input queue for scripted key sequences.
///
/// Events are sorted by frame number and processed at the start of each frame.
pub struct InputQueue {
    events: VecDeque<InputEvent>,
}

impl InputQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    /// Enqueue a raw input event.
    pub fn push(&mut self, event: InputEvent) {
        let pos = self
            .events
            .iter()
            .position(|e| e.frame > event.frame)
            .unwrap_or(self.events.len());
        self.events.insert(pos, event);
    }

    /// Enqueue a key press and release.
    pub fn enqueue_key(&mut self, key: C64Key, at_frame: u64, hold_frames: u64) {
        self.push(InputEvent {
            frame: at_frame,
            key,
            pressed: true,
        });
        self.push(InputEvent {
            frame: at_frame + hold_frames,
            key,
            pressed: false,
        });
    }

    /// Enqueue typing a string.
    ///
    /// Each character is held for 3 frames with a 3-frame gap.
    /// Returns the next free frame after all characters are typed.
    pub fn enqueue_text(&mut self, text: &str, start_frame: u64) -> u64 {
        let hold = 3u64;
        let gap = 3u64;
        let mut frame = start_frame;

        for ch in text.chars() {
            let keys = char_to_keys(ch);
            for &key in &keys {
                self.push(InputEvent {
                    frame,
                    key,
                    pressed: true,
                });
                self.push(InputEvent {
                    frame: frame + hold,
                    key,
                    pressed: false,
                });
            }
            frame += hold + gap;
        }

        frame
    }

    /// Process all events for the given frame, applying them to the keyboard.
    pub fn process(&mut self, frame: u64, keyboard: &mut KeyboardMatrix) {
        while let Some(event) = self.events.front() {
            if event.frame > frame {
                break;
            }
            let event = self.events.pop_front().expect("front was Some");
            let (row, col) = event.key.matrix();
            keyboard.set_key(row, col, event.pressed);
        }
    }

    /// Number of pending events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a character to the C64 keys needed to type it.
///
/// Returns 1 key for simple characters, 2 for shifted combinations.
/// The C64 keyboard maps uppercase letters as unshifted (the C64 normally
/// shows PETSCII uppercase), and shifted letters produce graphics chars.
fn char_to_keys(ch: char) -> Vec<C64Key> {
    match ch {
        'a' | 'A' => vec![C64Key::A],
        'b' | 'B' => vec![C64Key::B],
        'c' | 'C' => vec![C64Key::C],
        'd' | 'D' => vec![C64Key::D],
        'e' | 'E' => vec![C64Key::E],
        'f' | 'F' => vec![C64Key::F],
        'g' | 'G' => vec![C64Key::G],
        'h' | 'H' => vec![C64Key::H],
        'i' | 'I' => vec![C64Key::I],
        'j' | 'J' => vec![C64Key::J],
        'k' | 'K' => vec![C64Key::K],
        'l' | 'L' => vec![C64Key::L],
        'm' | 'M' => vec![C64Key::M],
        'n' | 'N' => vec![C64Key::N],
        'o' | 'O' => vec![C64Key::O],
        'p' | 'P' => vec![C64Key::P],
        'q' | 'Q' => vec![C64Key::Q],
        'r' | 'R' => vec![C64Key::R],
        's' | 'S' => vec![C64Key::S],
        't' | 'T' => vec![C64Key::T],
        'u' | 'U' => vec![C64Key::U],
        'v' | 'V' => vec![C64Key::V],
        'w' | 'W' => vec![C64Key::W],
        'x' | 'X' => vec![C64Key::X],
        'y' | 'Y' => vec![C64Key::Y],
        'z' | 'Z' => vec![C64Key::Z],
        '0' => vec![C64Key::N0],
        '1' => vec![C64Key::N1],
        '2' => vec![C64Key::N2],
        '3' => vec![C64Key::N3],
        '4' => vec![C64Key::N4],
        '5' => vec![C64Key::N5],
        '6' => vec![C64Key::N6],
        '7' => vec![C64Key::N7],
        '8' => vec![C64Key::N8],
        '9' => vec![C64Key::N9],
        ' ' => vec![C64Key::Space],
        '\n' => vec![C64Key::Return],
        '.' => vec![C64Key::Period],
        ',' => vec![C64Key::Comma],
        ':' => vec![C64Key::Colon],
        ';' => vec![C64Key::Semicolon],
        '=' => vec![C64Key::Equals],
        '/' => vec![C64Key::Slash],
        '+' => vec![C64Key::Plus],
        '-' => vec![C64Key::Minus],
        '*' => vec![C64Key::Asterisk],
        '@' => vec![C64Key::At],
        '"' => vec![C64Key::LShift, C64Key::N2],
        '(' => vec![C64Key::LShift, C64Key::N9],
        ')' => vec![C64Key::LShift, C64Key::N0],
        '>' => vec![C64Key::LShift, C64Key::Period],
        '<' => vec![C64Key::LShift, C64Key::Comma],
        '?' => vec![C64Key::LShift, C64Key::Slash],
        '$' => vec![C64Key::LShift, C64Key::N4],
        '!' => vec![C64Key::LShift, C64Key::N1],
        '#' => vec![C64Key::LShift, C64Key::N3],
        '%' => vec![C64Key::LShift, C64Key::N5],
        '&' => vec![C64Key::LShift, C64Key::N6],
        '\'' => vec![C64Key::LShift, C64Key::N7],
        _ => vec![], // Unsupported character — silently skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_positions() {
        assert_eq!(C64Key::Return.matrix(), (1, 0));
        assert_eq!(C64Key::Space.matrix(), (4, 7));
        assert_eq!(C64Key::A.matrix(), (2, 1));
        assert_eq!(C64Key::LShift.matrix(), (7, 1));
        assert_eq!(C64Key::RShift.matrix(), (4, 6));
    }

    #[test]
    fn enqueue_key_creates_press_and_release() {
        let mut queue = InputQueue::new();
        queue.enqueue_key(C64Key::A, 10, 3);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn enqueue_text_basic() {
        let mut queue = InputQueue::new();
        let next = queue.enqueue_text("AB", 0);
        assert_eq!(next, 12);
        assert_eq!(queue.len(), 4);
    }

    #[test]
    fn process_applies_events() {
        let mut queue = InputQueue::new();
        let mut kbd = KeyboardMatrix::new();

        queue.enqueue_key(C64Key::A, 5, 3);

        // Frame 4: nothing
        queue.process(4, &mut kbd);
        assert_eq!(kbd.scan(0xFB) & 0x02, 0x02); // A not pressed (row 2, col 1)

        // Frame 5: press
        queue.process(5, &mut kbd);
        assert_eq!(kbd.scan(0xFB) & 0x02, 0x00); // A pressed

        // Frame 8: release
        queue.process(8, &mut kbd);
        assert_eq!(kbd.scan(0xFB) & 0x02, 0x02); // A released
    }

    #[test]
    fn char_to_keys_shifted() {
        let keys = char_to_keys('"');
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], C64Key::LShift);
        assert_eq!(keys[1], C64Key::N2);
    }
}
