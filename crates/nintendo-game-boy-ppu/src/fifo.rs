//! The two 8-pixel shift registers of the DMG pixel pipeline.
//!
//! The background FIFO only ever takes a whole tile row, and only when
//! it is empty; the object FIFO is padded to eight entries and objects
//! are overlaid into the slots that are still transparent. Pan Docs
//! "Pixel FIFO"; SameBoy `Core/display.c` (`fifo_push_bg_row`,
//! `fifo_overlay_object_row`).

use serde::{Deserialize, Serialize};

const LEN: u8 = 8;

/// One pixel in flight: its 2-bit colour index plus, for object
/// pixels, the palette select (`OBP0`/`OBP1`) and OAM attribute bit 7.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct FifoItem {
    pub pixel: u8,
    pub palette: u8,
    pub bg_priority: bool,
}

/// Circular buffer of up to eight pixels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Fifo {
    data: [FifoItem; LEN as usize],
    head: u8,
    len: u8,
}

impl Fifo {
    pub(crate) const fn new() -> Self {
        Self {
            data: [FifoItem {
                pixel: 0,
                palette: 0,
                bg_priority: false,
            }; LEN as usize],
            head: 0,
            len: 0,
        }
    }

    fn slot(&self, offset: u8) -> usize {
        usize::from(self.head.wrapping_add(offset) % LEN)
    }

    /// Replaces the (empty) FIFO with one tile row, leftmost pixel
    /// first. `low`/`high` are the two bitplanes as fetched from VRAM.
    pub(crate) fn push_bg_row(&mut self, mut low: u8, mut high: u8) {
        self.head = 0;
        self.len = LEN;
        for item in &mut self.data {
            *item = FifoItem {
                pixel: (low >> 7) | ((high >> 7) << 1),
                palette: 0,
                bg_priority: false,
            };
            low <<= 1;
            high <<= 1;
        }
    }

    /// Replaces the (empty) FIFO with a single colour-0 pixel. This is
    /// the DMG window "pixel insertion" glitch.
    pub(crate) fn push_single_blank(&mut self) {
        self.head = 0;
        self.len = 1;
        self.data[0] = FifoItem::default();
    }

    /// Overlays one object row. The FIFO is first padded to eight
    /// transparent pixels; an object pixel only lands where the slot
    /// is still transparent, so the object fetched first wins (DMG
    /// priority is by X, then OAM index, which is the fetch order).
    pub(crate) fn overlay_object_row(
        &mut self,
        mut low: u8,
        mut high: u8,
        palette: u8,
        bg_priority: bool,
        flip_x: bool,
    ) {
        while self.len < LEN {
            let slot = self.slot(self.len);
            self.data[slot] = FifoItem::default();
            self.len += 1;
        }
        let flip_xor = if flip_x { 0 } else { 7 };
        for i in (0..LEN).rev() {
            let pixel = (low >> 7) | ((high >> 7) << 1);
            let slot = self.slot(i ^ flip_xor);
            let target = &mut self.data[slot];
            if pixel != 0 && target.pixel == 0 {
                *target = FifoItem {
                    pixel,
                    palette,
                    bg_priority,
                };
            }
            low <<= 1;
            high <<= 1;
        }
    }

    /// Removes and returns the leftmost pixel. Returns a transparent
    /// pixel if the FIFO is empty (callers check [`Fifo::len`] first).
    pub(crate) fn pop(&mut self) -> FifoItem {
        if self.len == 0 {
            return FifoItem::default();
        }
        let item = self.data[usize::from(self.head % LEN)];
        self.head = (self.head + 1) % LEN;
        self.len -= 1;
        item
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
    fn bg_row_pops_leftmost_pixel_first() {
        let mut fifo = Fifo::new();
        // Bit 7: high=1, low=1 -> 3; bit 6: high=1, low=0 -> 2; ...
        fifo.push_bg_row(0b1010_1010, 0b1100_1100);
        assert_eq!(fifo.len(), 8);
        let pixels: Vec<u8> = (0..8).map(|_| fifo.pop().pixel).collect();
        assert_eq!(pixels, [3, 2, 1, 0, 3, 2, 1, 0]);
        assert_eq!(fifo.len(), 0);
    }

    #[test]
    fn object_overlay_keeps_the_first_opaque_pixel() {
        let mut fifo = Fifo::new();
        fifo.overlay_object_row(0b1000_0000, 0, 0, false, false);
        fifo.overlay_object_row(0b1100_0000, 0b1100_0000, 1, true, false);
        let first = fifo.pop();
        assert_eq!(
            (first.pixel, first.palette, first.bg_priority),
            (1, 0, false)
        );
        let second = fifo.pop();
        assert_eq!(
            (second.pixel, second.palette, second.bg_priority),
            (3, 1, true)
        );
    }

    #[test]
    fn object_overlay_honours_x_flip() {
        let mut fifo = Fifo::new();
        fifo.overlay_object_row(0b1000_0000, 0, 0, false, true);
        let pixels: Vec<u8> = (0..8).map(|_| fifo.pop().pixel).collect();
        assert_eq!(pixels, [0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn overlay_pads_a_partly_drained_fifo() {
        let mut fifo = Fifo::new();
        fifo.overlay_object_row(0xFF, 0, 0, false, false);
        for _ in 0..5 {
            fifo.pop();
        }
        fifo.overlay_object_row(0xFF, 0xFF, 1, false, false);
        let pixels: Vec<u8> = (0..8).map(|_| fifo.pop().pixel).collect();
        assert_eq!(pixels, [1, 1, 1, 3, 3, 3, 3, 3]);
    }
}
