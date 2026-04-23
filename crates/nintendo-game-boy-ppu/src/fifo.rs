//! 16-slot pixel FIFO. Holds 2-bit BG/window pixel indices.

use serde::{Deserialize, Serialize};

/// Circular buffer of up to 16 two-bit pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Fifo {
    data: [u8; 16],
    head: u8,
    len: u8,
}

impl Fifo {
    pub(crate) const fn new() -> Self {
        Self {
            data: [0; 16],
            head: 0,
            len: 0,
        }
    }

    /// Push 8 pixels in display order (leftmost first).
    pub(crate) fn push8(&mut self, pixels: [u8; 8]) {
        for &p in &pixels {
            let slot = (self.head.wrapping_add(self.len) & 0xF) as usize;
            self.data[slot] = p & 0b11;
            self.len += 1;
        }
    }

    pub(crate) fn pop(&mut self) -> u8 {
        let p = self.data[(self.head & 0xF) as usize];
        self.head = self.head.wrapping_add(1);
        self.len -= 1;
        p
    }

    pub(crate) fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    pub(crate) const fn len(&self) -> u8 {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_round_trip() {
        let mut fifo = Fifo::new();
        fifo.push8([3, 2, 1, 0, 3, 2, 1, 0]);
        assert_eq!(fifo.len(), 8);
        assert_eq!(fifo.pop(), 3);
        assert_eq!(fifo.pop(), 2);
        assert_eq!(fifo.len(), 6);
    }

    #[test]
    fn fills_to_sixteen() {
        let mut fifo = Fifo::new();
        fifo.push8([1; 8]);
        fifo.push8([2; 8]);
        assert_eq!(fifo.len(), 16);
        assert_eq!(fifo.pop(), 1);
        assert_eq!(fifo.pop(), 1);
    }

    #[test]
    fn clear_resets_state() {
        let mut fifo = Fifo::new();
        fifo.push8([1; 8]);
        fifo.clear();
        assert_eq!(fifo.len(), 0);
    }
}
