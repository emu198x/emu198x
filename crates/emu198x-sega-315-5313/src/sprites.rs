//! Sprites: found, placed and drawn in their own slots.
//!
//! The sprite engine runs three jobs across a line, each in slots of its
//! own:
//!
//! - **Scan.** In H-blank the chip walks the sprite list, two sprites a
//!   slot, through its internal copy of each sprite's Y, size and link
//!   fields, and notes the sprites on the next line, up to 20 (H40) or 16
//!   (H32). One more sets the overflow flag.
//! - **X read.** In each column of the picture, one sprite slot reads the
//!   next noted sprite's X and pattern from VRAM, giving the row of its
//!   pattern to draw.
//! - **Render.** From the end of the picture through H-blank, each render
//!   slot draws one eight-pixel cell into the sprite line buffer: 40 cells a
//!   line in H40, 32 in H32. A sprite at X 0 after one that drew stops the
//!   rest of the line's sprites (masking). A pixel landing on one already
//!   drawn sets the collision flag.
//!
//! So a sprite drawn on line N was found on line N-2's blank, placed during
//! line N-1's columns and drawn from line N-1's end; that is why limits,
//! masking and mid-frame changes to the sprite table behave as they do.

use serde::{Deserialize, Serialize};

use crate::Vdp;
use crate::timing::Width;

/// Sprites the scan can note for one line.
pub(crate) const MAX_PER_LINE: usize = 20;
const MAX_PER_LINE_H32: u8 = 16;
/// Sprites in the list.
const MAX_PER_FRAME: u8 = 80;
/// H32 addresses only the first 64.
const MAX_PER_FRAME_H32: u8 = 64;

/// A sprite placed for the next line's render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SpriteDraw {
    /// The pattern row's VRAM address.
    address: u16,
    /// Raw X, 0-511; 128 is the left edge. Zero once drawn.
    x: u16,
    attributes: u8,
    h_flip: bool,
    /// Width in cells and height in lines.
    width: u8,
    height: u8,
}

/// A sprite the scan found: its size byte and list index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SpriteFound {
    size: u8,
    index: u8,
}

/// The sprite engine's state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Sprites {
    /// The next list entry to scan, with bit 7 set while scanning.
    index: u8,
    /// Sprites found for the next line.
    found_count: u8,
    found: [SpriteFound; MAX_PER_LINE],
    /// The draw list, filled from the top down.
    draws: [SpriteDraw; MAX_PER_LINE],
    /// Free draw entries left this line.
    draws_left: u8,
    /// The entry being read or drawn.
    cursor: i8,
    /// The cell of the current sprite being drawn.
    cell: u8,
    /// A sprite has drawn on this line, so an X-0 sprite masks.
    can_mask: bool,
    masked: bool,
}

impl Vdp {
    fn max_per_line(&self) -> u8 {
        if self.width() == Width::H40 {
            MAX_PER_LINE as u8
        } else {
            MAX_PER_LINE_H32
        }
    }

    /// The sprite attribute table's VRAM address.
    fn sat_address(&self) -> usize {
        let mut address = usize::from(self.regs[0x05]) << 9;
        address &= 0xFFFF;
        if self.width() == Width::H40 {
            address &= 0x1FC00;
        }
        address
    }

    /// The sprite work in sprite slot `hslot`: begin the render, draw a
    /// cell, scan, or a combination.
    pub(crate) fn sprite_slot(&mut self, width: Width, hslot: u8) {
        let (begin, render_from, scan_start) = match width {
            Width::H40 => (163, 163..=166, 167),
            Width::H32 => (131, 131..=134, 135),
        };
        if hslot == begin {
            self.begin_sprite_render();
        }
        let scans = match width {
            Width::H40 => {
                matches!(hslot, 167..=182 | 229..=231 | 233..=243 | 245..=248 | 250 | 254)
            }
            Width::H32 => {
                matches!(hslot, 135..=144 | 146 | 147 | 233..=242 | 245..=248 | 250 | 254)
            }
        };
        if render_from.contains(&hslot) || scans {
            if hslot == scan_start {
                self.sprites.index = 0x80;
                self.sprites.found_count = 0;
            }
            self.render_sprite_cell();
        }
        if scans {
            self.scan_sprites();
        }
    }

    /// The end of the picture: the line buffer is cleared for the next
    /// line, and the render starts at the top of the draw list.
    fn begin_sprite_render(&mut self) {
        self.sprites.cursor = self.max_per_line() as i8 - 1;
        self.sprite_line.fill(0);
        self.sprites.masked = false;
        while self.sprites.draws_left > 0 {
            self.sprites.draws_left -= 1;
            self.sprites.draws[usize::from(self.sprites.draws_left)].x = 0;
        }
    }

    /// The start of a line's columns: the X reads begin with the first
    /// sprite found.
    pub(crate) fn begin_sprite_reads(&mut self) {
        self.sprites.cursor = self.sprites.found_count as i8;
        self.sprites.cell = 0;
        self.sprites.draws_left = self.max_per_line();
    }

    /// On a blank line: the buffer is cleared and the scan index reset at
    /// the slots the picture would use, though nothing is drawn.
    pub(crate) fn blank_sprite_slot(&mut self, width: Width, hslot: u8) {
        let (clear, reset) = match width {
            Width::H40 => (163, 167),
            Width::H32 => (128, 132),
        };
        if hslot == clear {
            self.sprites.cursor = self.max_per_line() as i8 - 1;
            self.sprite_line.fill(0);
        } else if hslot == reset {
            self.sprites.index = 0x80;
            self.sprites.found_count = 0;
        }
    }

    /// The picture ends: the render cursor goes to the top.
    pub(crate) fn end_sprite_picture(&mut self) {
        self.sprites.cursor = MAX_PER_LINE as i8 - 1;
        self.sprites.cell = 0;
    }

    /// The line the scan and X reads work for, and the Y mask and offset
    /// that sprite coordinates use.
    fn sprite_line_number(&self) -> (u16, u16, u16, u8) {
        let mut line = self.vcounter + 1;
        if self.double_res() {
            line *= 2;
            if self.even_field {
                line += 1;
            }
            (line, 0x3FF, 256, 16)
        } else {
            (line, 0x1FF, 128, 8)
        }
    }

    /// Scan two sprites of the list.
    pub(crate) fn scan_sprites(&mut self) {
        let max_line = self.max_per_line();
        if self.sprites.index == 0 || (self.sprites.found_count >= max_line && self.sprite_overflow)
        {
            return;
        }
        let (line, ymask, ymin, height_unit) = self.sprite_line_number();
        let line = (line + ymin) & ymask;
        self.sprites.index &= 0x7F;
        for second in [false, true] {
            if second && (self.sprites.index == 0 || self.sprites.found_count >= max_line) {
                return;
            }
            if self.sprites.index >= MAX_PER_FRAME {
                self.sprites.index = 0;
                return;
            }
            let cache = usize::from(self.sprites.index) * 4;
            let y = ((u16::from(self.sat_cache[cache] & 3) << 8)
                | u16::from(self.sat_cache[cache + 1]))
                & ymask;
            let height = (u16::from(self.sat_cache[cache + 2] & 3) + 1) * u16::from(height_unit);
            if y <= line && line < y + height {
                if self.sprites.found_count == max_line {
                    self.sprite_overflow = true;
                    return;
                }
                let slot = usize::from(self.sprites.found_count);
                self.sprites.found[slot] = SpriteFound {
                    size: self.sat_cache[cache + 2],
                    index: self.sprites.index,
                };
                self.sprites.found_count += 1;
            }
            self.sprites.index = self.sat_cache[cache + 3] & 0x7F;
        }
    }

    /// Read the next found sprite's X and pattern row into the draw list.
    pub(crate) fn sprite_x_slot(&mut self) {
        let max_line = self.max_per_line() as i8;
        if self.sprites.cursor == max_line {
            self.sprites.cursor = 0;
        }
        let cursor = self.sprites.cursor;
        if cursor >= 0 && cursor < self.sprites.found_count as i8 && self.sprites.draws_left > 0 {
            let found = self.sprites.found[cursor as usize];
            let (line, ymask, ymin, _) = self.sprite_line_number();
            let width = ((found.size >> 2) & 3) + 1;
            let mut height = ((found.size & 3) + 1) * 8;
            if self.double_res() {
                height *= 2;
            }
            let mut index = found.index;
            if self.width() == Width::H32 {
                index &= MAX_PER_FRAME_H32 - 1;
            }
            let attr = self.sat_address() + usize::from(index) * 8 + 4;
            let byte = |i: usize| self.vram[(attr + i) & 0xFFFF];
            let tile = u16::from(byte(0)) << 8 | u16::from(byte(1));
            let line = (line + ymin) & ymask;
            let cache = usize::from(found.index) * 4;
            let y = (u16::from(self.sat_cache[cache]) << 8 | u16::from(self.sat_cache[cache + 1]))
                & ymask;
            let row = if tile & 0x1000 != 0 {
                (y + u16::from(height) - 1).wrapping_sub(line)
            } else {
                line.wrapping_sub(y)
            } & (ymask >> 4);
            let address = if self.double_res() {
                ((tile & 0x3FF) << 6).wrapping_add(row * 4)
            } else {
                ((tile & 0x7FF) << 5).wrapping_add(row * 4)
            };
            self.sprites.draws_left -= 1;
            self.sprites.draws[usize::from(self.sprites.draws_left)] = SpriteDraw {
                address,
                x: (u16::from(byte(2) & 3) << 8 | u16::from(byte(3))) & 0x1FF,
                attributes: ((tile >> 9) & 0x70) as u8,
                h_flip: tile & 0x0800 != 0,
                width,
                height,
            };
        }
        self.sprites.cursor += 1;
    }

    /// Draw one cell of the current sprite into the line buffer.
    fn render_sprite_cell(&mut self) {
        if self.sprites.cursor < 0 {
            self.sprites.cursor += 32;
        }
        if self.sprites.cursor as usize >= MAX_PER_LINE {
            self.sprites.cursor -= 1;
            return;
        }
        let draw = self.sprites.draws[self.sprites.cursor as usize];
        let address = draw
            .address
            .wrapping_add(u16::from(self.sprites.cell) * u16::from(draw.height) * 4);
        if draw.x != 0 {
            let (mut x, step): (i32, i32) = if draw.h_flip {
                (
                    i32::from(draw.x)
                        + 7
                        + 8 * (i32::from(draw.width) - i32::from(self.sprites.cell) - 1),
                    -1,
                )
            } else {
                (i32::from(draw.x) + i32::from(self.sprites.cell) * 8, 1)
            };
            self.sprites.can_mask = true;
            if !self.sprites.masked {
                x -= 128;
                let mut collide = 0;
                if x > -8 && x < 327 {
                    for i in 0..4u16 {
                        let byte = self.vram[usize::from(address.wrapping_add(i))];
                        for pixel in [byte >> 4, byte & 0xF] {
                            if (0..320).contains(&x) {
                                let slot = &mut self.sprite_line[x as usize];
                                if *slot & 0xF == 0 {
                                    *slot = pixel | draw.attributes;
                                } else {
                                    collide |= pixel;
                                }
                            }
                            x += step;
                        }
                    }
                }
                if collide != 0 {
                    self.sprite_collision = true;
                }
            }
        } else if self.sprites.can_mask {
            self.sprites.masked = true;
            self.sprites.can_mask = false;
        }
        self.sprites.cell += 1;
        if self.sprites.cell >= draw.width {
            self.sprites.draws[self.sprites.cursor as usize].x = 0;
            self.sprites.cell = 0;
            self.sprites.cursor -= 1;
        }
    }
}
