//! The standard controller: a 16-bit shift register behind each port.
//!
//! Latch high reloads it from the buttons; each read of the data line then
//! returns one button and shifts, in the order B, Y, Select, Start, Up,
//! Down, Left, Right, A, X, L, R, then four zero bits that identify a
//! standard pad. Past sixteen reads the line stays high. A pressed button
//! reads as 1, since the console inverts the line.

use serde::{Deserialize, Serialize};

/// A pad's buttons, true while pressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buttons {
    pub b: bool,
    pub y: bool,
    pub select: bool,
    pub start: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub x: bool,
    pub l: bool,
    pub r: bool,
}

impl Buttons {
    /// The sixteen bits in shift order, first bit in bit 15.
    #[must_use]
    pub fn word(self) -> u16 {
        [
            self.b,
            self.y,
            self.select,
            self.start,
            self.up,
            self.down,
            self.left,
            self.right,
            self.a,
            self.x,
            self.l,
            self.r,
        ]
        .into_iter()
        .enumerate()
        .fold(0u16, |word, (i, pressed)| {
            word | u16::from(pressed) << (15 - i)
        })
    }
}

/// One controller port with a standard pad in it, or nothing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pad {
    pub connected: bool,
    pub buttons: Buttons,
    latched: bool,
    counter: u8,
}

impl Pad {
    #[must_use]
    pub fn new(connected: bool) -> Self {
        Self {
            connected,
            ..Self::default()
        }
    }

    /// Drive the latch line.
    pub fn latch(&mut self, level: bool) {
        if self.latched == level {
            return;
        }
        self.latched = level;
        self.counter = 0;
    }

    /// Clock the data line: bit 0 is the pad's data, bit 1 the second data
    /// line a multitap would use. An empty port reads 0.
    pub fn data(&mut self) -> u8 {
        if !self.connected {
            return 0;
        }
        if self.counter >= 16 {
            return 1;
        }
        let word = self.buttons.word();
        if self.latched {
            return u8::from(word & 0x8000 != 0);
        }
        let bit = word >> (15 - self.counter) & 1;
        self.counter += 1;
        bit as u8
    }
}
