//! Background / window pixel fetcher.
//!
//! 4-state machine, 2 dots per state:
//! `read_tile_id → read_tile_data_low → read_tile_data_high → push`.
//! On each `push` (when the FIFO has at most 8 pixels) it commits a
//! row of 8 pixels and increments the tile column counter.

use serde::{Deserialize, Serialize};

use crate::fifo::Fifo;

/// Per-tick context passed in from the PPU. Bundles the live
/// register values the fetcher needs so the function signature stays
/// readable (and clippy happy about the argument count).
#[derive(Clone, Copy)]
pub(crate) struct FetchCtx<'a> {
    pub lcdc: u8,
    pub ly: u8,
    pub scx: u8,
    pub scy: u8,
    pub window_line: u8,
    /// Allows borrowing future PPU state without changing the
    /// signature; not consumed today.
    pub _marker: core::marker::PhantomData<&'a ()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum State {
    ReadTileId,
    ReadTileDataLow,
    ReadTileDataHigh,
    Push,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct Fetcher {
    state: State,
    /// Each state takes 2 dots — the first dot is "wait", the second
    /// performs the work. `ticks` toggles between 0 and 1.
    ticks: u8,
    tile_id: u8,
    tile_data_low: u8,
    tile_data_high: u8,
    /// Tile column counter within the current scanline.
    x: u8,
    window_mode: bool,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetcher {
    pub(crate) const fn new() -> Self {
        Self {
            state: State::ReadTileId,
            ticks: 0,
            tile_id: 0,
            tile_data_low: 0,
            tile_data_high: 0,
            x: 0,
            window_mode: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.state = State::ReadTileId;
        self.ticks = 0;
        self.x = 0;
        self.window_mode = false;
    }

    pub(crate) fn switch_to_window(&mut self) {
        self.state = State::ReadTileId;
        self.ticks = 0;
        self.x = 0;
        self.window_mode = true;
    }

    pub(crate) const fn is_window(&self) -> bool {
        self.window_mode
    }

    /// Advance the fetcher by one dot. The work for each state lands
    /// on the second dot of the pair (after the wait dot).
    pub(crate) fn tick(&mut self, ctx: FetchCtx<'_>, fifo: &mut Fifo, vram: &[u8]) {
        self.ticks ^= 1;
        if self.ticks != 0 {
            return; // first dot of the pair: wait
        }

        match self.state {
            State::ReadTileId => {
                let map_addr = if self.window_mode {
                    let map_base: u16 = if (ctx.lcdc & 0x40) != 0 { 0x1C00 } else { 0x1800 };
                    let wy_row = u16::from(ctx.window_line) / 8;
                    map_base + wy_row * 32 + u16::from(self.x)
                } else {
                    let y = ctx.ly.wrapping_add(ctx.scy);
                    let x = (self.x as u16) * 8 + u16::from(ctx.scx);
                    let map_base: u16 = if (ctx.lcdc & 0x08) != 0 { 0x1C00 } else { 0x1800 };
                    map_base + (u16::from(y) / 8 % 32) * 32 + (x / 8 % 32)
                };
                self.tile_id = vram[usize::from(map_addr)];
                self.state = State::ReadTileDataLow;
            }
            State::ReadTileDataLow => {
                self.tile_data_low = vram[usize::from(self.tile_data_addr(ctx))];
                self.state = State::ReadTileDataHigh;
            }
            State::ReadTileDataHigh => {
                self.tile_data_high = vram[usize::from(self.tile_data_addr(ctx) + 1)];
                self.state = State::Push;
            }
            State::Push => {
                if fifo.len() <= 8 {
                    fifo.push8(self.decode_pixels());
                    self.x = self.x.wrapping_add(1);
                    self.state = State::ReadTileId;
                }
                // Else: stall, retry next 2-dot cycle.
            }
        }
    }

    fn tile_data_addr(&self, ctx: FetchCtx<'_>) -> u16 {
        let row: u16 = if self.window_mode {
            u16::from(ctx.window_line) % 8
        } else {
            u16::from(ctx.ly.wrapping_add(ctx.scy)) % 8
        };

        if (ctx.lcdc & 0x10) != 0 {
            // Unsigned: base $0000 ($8000 absolute), tile_id 0-255.
            u16::from(self.tile_id) * 16 + row * 2
        } else {
            // Signed: base $1000 ($9000 absolute), tile_id is signed.
            let signed_id = self.tile_id as i8;
            let addr = 0x1000_i32 + i32::from(signed_id) * 16 + i32::from(row) * 2;
            addr as u16
        }
    }

    pub(crate) fn decode_pixels(&self) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        for (i, slot) in pixels.iter_mut().enumerate() {
            let bit = 7 - i as u8;
            let low = (self.tile_data_low >> bit) & 1;
            let high = (self.tile_data_high >> bit) & 1;
            *slot = (high << 1) | low;
        }
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_pixels_combines_low_and_high() {
        let mut fetcher = Fetcher::new();
        fetcher.tile_data_low = 0b1010_1010;
        fetcher.tile_data_high = 0b1100_1100;
        let pixels = fetcher.decode_pixels();
        // Bit 7: high=1, low=1 -> 3
        // Bit 6: high=1, low=0 -> 2
        // Bit 5: high=0, low=1 -> 1
        // Bit 4: high=0, low=0 -> 0
        assert_eq!(pixels, [3, 2, 1, 0, 3, 2, 1, 0]);
    }
}
