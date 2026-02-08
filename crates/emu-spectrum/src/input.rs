//! Input handling for the ZX Spectrum.
//!
//! Three layers:
//! 1. `SpectrumKey` — logical key names mapped to the 8×5 keyboard matrix.
//! 2. Immediate `press_key`/`release_key` methods on `Spectrum`.
//! 3. `InputQueue` — timed key events for scripted sequences.

use std::collections::VecDeque;

use crate::keyboard::KeyboardState;

/// Logical key on the 48K Spectrum keyboard.
///
/// Each key maps to a (row, bit) pair in the 8×5 keyboard matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpectrumKey {
    // Row 0 (addr bit A8)
    CapsShift,
    Z,
    X,
    C,
    V,
    // Row 1 (addr bit A9)
    A,
    S,
    D,
    F,
    G,
    // Row 2 (addr bit A10)
    Q,
    W,
    E,
    R,
    T,
    // Row 3 (addr bit A11)
    N1,
    N2,
    N3,
    N4,
    N5,
    // Row 4 (addr bit A12)
    N0,
    N9,
    N8,
    N7,
    N6,
    // Row 5 (addr bit A13)
    P,
    O,
    I,
    U,
    Y,
    // Row 6 (addr bit A14)
    Enter,
    L,
    K,
    J,
    H,
    // Row 7 (addr bit A15)
    Space,
    SymShift,
    M,
    N,
    B,
}

impl SpectrumKey {
    /// Return the (row, bit) pair for this key in the keyboard matrix.
    #[must_use]
    pub const fn matrix(self) -> (usize, u8) {
        match self {
            Self::CapsShift => (0, 0),
            Self::Z => (0, 1),
            Self::X => (0, 2),
            Self::C => (0, 3),
            Self::V => (0, 4),

            Self::A => (1, 0),
            Self::S => (1, 1),
            Self::D => (1, 2),
            Self::F => (1, 3),
            Self::G => (1, 4),

            Self::Q => (2, 0),
            Self::W => (2, 1),
            Self::E => (2, 2),
            Self::R => (2, 3),
            Self::T => (2, 4),

            Self::N1 => (3, 0),
            Self::N2 => (3, 1),
            Self::N3 => (3, 2),
            Self::N4 => (3, 3),
            Self::N5 => (3, 4),

            Self::N0 => (4, 0),
            Self::N9 => (4, 1),
            Self::N8 => (4, 2),
            Self::N7 => (4, 3),
            Self::N6 => (4, 4),

            Self::P => (5, 0),
            Self::O => (5, 1),
            Self::I => (5, 2),
            Self::U => (5, 3),
            Self::Y => (5, 4),

            Self::Enter => (6, 0),
            Self::L => (6, 1),
            Self::K => (6, 2),
            Self::J => (6, 3),
            Self::H => (6, 4),

            Self::Space => (7, 0),
            Self::SymShift => (7, 1),
            Self::M => (7, 2),
            Self::N => (7, 3),
            Self::B => (7, 4),
        }
    }
}

/// A timed keyboard event.
#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Frame number at which this event fires.
    pub frame: u64,
    /// Which key.
    pub key: SpectrumKey,
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
        // Insert in sorted order by frame.
        let pos = self
            .events
            .iter()
            .position(|e| e.frame > event.frame)
            .unwrap_or(self.events.len());
        self.events.insert(pos, event);
    }

    /// Enqueue a key press and release.
    ///
    /// The key is pressed at `at_frame` and released at `at_frame + hold_frames`.
    pub fn enqueue_key(&mut self, key: SpectrumKey, at_frame: u64, hold_frames: u64) {
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
    pub fn process(&mut self, frame: u64, keyboard: &mut KeyboardState) {
        while let Some(event) = self.events.front() {
            if event.frame > frame {
                break;
            }
            let event = self.events.pop_front().expect("front was Some");
            let (row, bit) = event.key.matrix();
            keyboard.set_key(row, bit, event.pressed);
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

/// Map a character to the Spectrum keys needed to type it.
///
/// Returns 1 key for simple characters, 2 for shifted combinations.
fn char_to_keys(ch: char) -> Vec<SpectrumKey> {
    match ch {
        'a' | 'A' => vec![SpectrumKey::A],
        'b' | 'B' => vec![SpectrumKey::B],
        'c' | 'C' => vec![SpectrumKey::C],
        'd' | 'D' => vec![SpectrumKey::D],
        'e' | 'E' => vec![SpectrumKey::E],
        'f' | 'F' => vec![SpectrumKey::F],
        'g' | 'G' => vec![SpectrumKey::G],
        'h' | 'H' => vec![SpectrumKey::H],
        'i' | 'I' => vec![SpectrumKey::I],
        'j' | 'J' => vec![SpectrumKey::J],
        'k' | 'K' => vec![SpectrumKey::K],
        'l' | 'L' => vec![SpectrumKey::L],
        'm' | 'M' => vec![SpectrumKey::M],
        'n' | 'N' => vec![SpectrumKey::N],
        'o' | 'O' => vec![SpectrumKey::O],
        'p' | 'P' => vec![SpectrumKey::P],
        'q' | 'Q' => vec![SpectrumKey::Q],
        'r' | 'R' => vec![SpectrumKey::R],
        's' | 'S' => vec![SpectrumKey::S],
        't' | 'T' => vec![SpectrumKey::T],
        'u' | 'U' => vec![SpectrumKey::U],
        'v' | 'V' => vec![SpectrumKey::V],
        'w' | 'W' => vec![SpectrumKey::W],
        'x' | 'X' => vec![SpectrumKey::X],
        'y' | 'Y' => vec![SpectrumKey::Y],
        'z' | 'Z' => vec![SpectrumKey::Z],
        '0' => vec![SpectrumKey::N0],
        '1' => vec![SpectrumKey::N1],
        '2' => vec![SpectrumKey::N2],
        '3' => vec![SpectrumKey::N3],
        '4' => vec![SpectrumKey::N4],
        '5' => vec![SpectrumKey::N5],
        '6' => vec![SpectrumKey::N6],
        '7' => vec![SpectrumKey::N7],
        '8' => vec![SpectrumKey::N8],
        '9' => vec![SpectrumKey::N9],
        ' ' => vec![SpectrumKey::Space],
        '\n' => vec![SpectrumKey::Enter],
        '"' => vec![SpectrumKey::SymShift, SpectrumKey::P],
        ':' => vec![SpectrumKey::SymShift, SpectrumKey::Z],
        ';' => vec![SpectrumKey::SymShift, SpectrumKey::O],
        '-' => vec![SpectrumKey::SymShift, SpectrumKey::J],
        '+' => vec![SpectrumKey::SymShift, SpectrumKey::K],
        '*' => vec![SpectrumKey::SymShift, SpectrumKey::B],
        '/' => vec![SpectrumKey::SymShift, SpectrumKey::V],
        '=' => vec![SpectrumKey::SymShift, SpectrumKey::L],
        '<' => vec![SpectrumKey::SymShift, SpectrumKey::R],
        '>' => vec![SpectrumKey::SymShift, SpectrumKey::T],
        ',' => vec![SpectrumKey::SymShift, SpectrumKey::N],
        '.' => vec![SpectrumKey::SymShift, SpectrumKey::M],
        '(' => vec![SpectrumKey::SymShift, SpectrumKey::N8],
        ')' => vec![SpectrumKey::SymShift, SpectrumKey::N9],
        _ => vec![], // Unsupported character — silently skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matrix_rows() {
        // Spot-check a few keys
        assert_eq!(SpectrumKey::CapsShift.matrix(), (0, 0));
        assert_eq!(SpectrumKey::V.matrix(), (0, 4));
        assert_eq!(SpectrumKey::A.matrix(), (1, 0));
        assert_eq!(SpectrumKey::Enter.matrix(), (6, 0));
        assert_eq!(SpectrumKey::Space.matrix(), (7, 0));
        assert_eq!(SpectrumKey::SymShift.matrix(), (7, 1));
    }

    #[test]
    fn enqueue_key_creates_press_and_release() {
        let mut queue = InputQueue::new();
        queue.enqueue_key(SpectrumKey::A, 10, 3);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn process_applies_events() {
        let mut queue = InputQueue::new();
        let mut kbd = KeyboardState::new();

        queue.enqueue_key(SpectrumKey::A, 5, 3);

        // Frame 4: nothing happens
        queue.process(4, &mut kbd);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x01); // A not pressed

        // Frame 5: press
        queue.process(5, &mut kbd);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x00); // A pressed (active low)

        // Frame 8: release
        queue.process(8, &mut kbd);
        assert_eq!(kbd.read(0xFD) & 0x01, 0x01); // A released
    }

    #[test]
    fn enqueue_text_basic() {
        let mut queue = InputQueue::new();
        let next = queue.enqueue_text("AB", 0);
        // A: press at 0, release at 3, B: press at 6, release at 9
        // Next free frame = 12
        assert_eq!(next, 12);
        assert_eq!(queue.len(), 4); // 2 press + 2 release
    }

    #[test]
    fn char_to_keys_shifted() {
        let keys = char_to_keys('"');
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], SpectrumKey::SymShift);
        assert_eq!(keys[1], SpectrumKey::P);
    }
}
