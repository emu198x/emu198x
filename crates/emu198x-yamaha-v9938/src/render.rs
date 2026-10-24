//! The background: each mode's pixel at a given position, the border, and
//! the per-dot output that combines them with the sprite line.

use crate::{ACTIVE_WIDTH, Mode, V9938};

/// Graphic 7's sprites do not use the palette: they take these fixed
/// colours, `0x0GRB`.
const GRAPHIC7_SPRITE_PALETTE: [u16; 16] = [
    0x000, 0x002, 0x030, 0x032, 0x300, 0x302, 0x330, 0x332, 0x472, 0x007, 0x070, 0x077, 0x700,
    0x707, 0x770, 0x777,
];

impl V9938 {
    /// Framebuffer rows above the active area: the TMS9918 crate's border,
    /// less ten lines when the picture is 212 lines tall, because the chip
    /// starts the taller picture ten lines sooner.
    pub(crate) fn border_top(&self) -> usize {
        let base = self.region.border_top_192() as usize;
        if self.active_lines() == 212 {
            base - 10
        } else {
            base
        }
    }

    /// The backdrop colour at framebuffer column `column`.
    fn backdrop(&self, column: usize) -> u32 {
        let r7 = self.regs[7];
        match self.mode() {
            Mode::G7 => grb332_to_argb(r7),
            Mode::G5 => {
                // Graphic 5 alternates two 2-bit backdrop colours.
                let index = if column & 1 == 0 {
                    (r7 >> 2) & 3
                } else {
                    r7 & 3
                };
                Self::grb_to_argb(self.palette[usize::from(index)])
            }
            _ => Self::grb_to_argb(self.palette[usize::from(r7 & 0x0F)]),
        }
    }

    /// Paint this line's border from the live backdrop, as the TMS9918
    /// crate does: the top border as the frame opens, the side columns of an
    /// active line, and whole rows below the picture.
    pub(crate) fn paint_border_for_line(&mut self) {
        let width = self.region.framebuffer_width() as usize;
        let height = self.region.framebuffer_height() as usize;
        let top = self.border_top();
        let left = self.region.border_left() as usize;
        let line = usize::from(self.line);
        if line == 0 {
            for row in 0..top.min(height) {
                self.fill_row(row, 0, width);
            }
        }
        let row = top + line;
        if row >= height {
            return;
        }
        if line < usize::from(self.active_lines()) {
            self.fill_row(row, 0, left);
            self.fill_row(row, left + 2 * ACTIVE_WIDTH as usize, width);
        } else {
            self.fill_row(row, 0, width);
        }
    }

    fn fill_row(&mut self, row: usize, from: usize, to: usize) {
        let width = self.region.framebuffer_width() as usize;
        for column in from..to {
            let colour = self.backdrop(column);
            self.framebuffer[row * width + column] = colour;
        }
    }

    /// Draw the active pixel, or pair of pixels, scanned out at this dot.
    pub(crate) fn render_dot(&mut self) {
        let width = self.region.framebuffer_width() as usize;
        let row = self.border_top() + usize::from(self.line);
        if row >= self.region.framebuffer_height() as usize {
            return;
        }
        let x = usize::from(self.dot);
        let column = self.region.border_left() as usize + 2 * x;
        let mode = self.mode();
        let (left, right) = if self.regs[1] & 0x40 == 0 || mode == Mode::Undefined {
            (self.backdrop(column), self.backdrop(column + 1))
        } else {
            let y = (usize::from(self.line) + usize::from(self.regs[23])) & 0xFF;
            let sprite = self.sprite_line[x];
            if sprite & 0x80 != 0 {
                let colour = self.sprite_colour(mode, sprite & 0x0F);
                (colour, colour)
            } else if mode.is_hires() {
                (
                    self.colour(mode, self.hires_pixel(mode, 2 * x, y), column),
                    self.colour(mode, self.hires_pixel(mode, 2 * x + 1, y), column + 1),
                )
            } else {
                let colour = self.colour(mode, self.lowres_pixel(mode, x, y), column);
                (colour, colour)
            }
        };
        let at = row * width + column;
        self.framebuffer[at] = left;
        self.framebuffer[at + 1] = right;
    }

    /// A background pixel as ARGB32. Colour 0 is the backdrop unless
    /// `R#8`'s TP bit makes it solid.
    fn colour(&self, mode: Mode, value: u8, column: usize) -> u32 {
        let transparent = self.regs[8] & 0x20 == 0;
        if value == 0 && transparent {
            return self.backdrop(column);
        }
        if mode == Mode::G7 {
            grb332_to_argb(value)
        } else {
            Self::grb_to_argb(self.palette[usize::from(value & 0x0F)])
        }
    }

    fn sprite_colour(&self, mode: Mode, index: u8) -> u32 {
        if mode == Mode::G7 {
            Self::grb_to_argb(GRAPHIC7_SPRITE_PALETTE[usize::from(index)])
        } else {
            Self::grb_to_argb(self.palette[usize::from(index)])
        }
    }

    fn vram_at(&self, logical: u32, planar: bool) -> u8 {
        if planar {
            self.vram[Self::planar(logical)]
        } else {
            self.vram[(logical as usize) & 0x1FFFF]
        }
    }

    /// The name table base: `R#2` supplies address bits 10-16.
    fn name_base(&self) -> u32 {
        u32::from(self.regs[2] & 0x7F) << 10
    }

    /// A pixel in a 256-wide mode at screen `x` and scrolled line `y`.
    pub(crate) fn lowres_pixel(&self, mode: Mode, x: usize, y: usize) -> u8 {
        let x32 = x as u32;
        let y32 = y as u32;
        match mode {
            Mode::G1 => {
                let name = self.vram_at(self.name_base() | ((y32 / 8) * 32 + x32 / 8), false);
                let pattern_base = u32::from(self.regs[4] & 0x3F) << 11;
                let pattern = self.vram_at(pattern_base | (u32::from(name) * 8 + (y32 & 7)), false);
                let colour_base = (u32::from(self.regs[10]) << 14) | (u32::from(self.regs[3]) << 6);
                let colour = self.vram_at(colour_base | u32::from(name >> 3), false);
                pick(pattern, x, colour)
            }
            Mode::G2 | Mode::G3 => {
                let name = self.vram_at(self.name_base() | ((y32 / 8) * 32 + x32 / 8), false);
                let index = ((y32 / 64) << 8) | u32::from(name);
                let offset = index * 8 + (y32 & 7);
                let r4 = u32::from(self.regs[4]);
                let pattern_address = ((r4 & 0x3C) << 11) | (offset & (((r4 & 3) << 11) | 0x7FF));
                let r3 = u32::from(self.regs[3]);
                let colour_address = (u32::from(self.regs[10]) << 14)
                    | ((r3 & 0x80) << 6)
                    | (offset & (((r3 & 0x7F) << 6) | 0x3F));
                let pattern = self.vram_at(pattern_address, false);
                let colour = self.vram_at(colour_address, false);
                pick(pattern, x, colour)
            }
            Mode::Mc => {
                let name = self.vram_at(self.name_base() | ((y32 / 8) * 32 + x32 / 8), false);
                let pattern_base = u32::from(self.regs[4] & 0x3F) << 11;
                let block = self.vram_at(
                    pattern_base | (u32::from(name) * 8 + ((y32 / 8) & 3) * 2 + ((y32 / 4) & 1)),
                    false,
                );
                if x & 4 == 0 { block >> 4 } else { block & 0x0F }
            }
            Mode::T1 => {
                // 40 columns of six pixels, with eight pixels of background
                // either side.
                if !(8..248).contains(&x) {
                    return self.regs[7] & 0x0F;
                }
                let column = (x32 - 8) / 6;
                let bit = (x - 8) % 6;
                let name = self.vram_at(self.name_base() | ((y32 / 8) * 40 + column), false);
                let pattern_base = u32::from(self.regs[4] & 0x3F) << 11;
                let pattern = self.vram_at(pattern_base | (u32::from(name) * 8 + (y32 & 7)), false);
                if pattern & (0x80 >> bit) != 0 {
                    self.regs[7] >> 4
                } else {
                    self.regs[7] & 0x0F
                }
            }
            Mode::G4 => {
                let base = u32::from(self.regs[2] & 0x60) << 10;
                let byte = self.vram_at(base | (y32 * 128 + x32 / 2), false);
                if x & 1 == 0 { byte >> 4 } else { byte & 0x0F }
            }
            Mode::G7 => {
                let base = u32::from(self.regs[2] & 0x20) << 11;
                self.vram_at(base | (y32 * 256 + x32), true)
            }
            _ => 0,
        }
    }

    /// A pixel in a 512-wide mode at hi-res column `hx`.
    pub(crate) fn hires_pixel(&self, mode: Mode, hx: usize, y: usize) -> u8 {
        let hx32 = hx as u32;
        let y32 = y as u32;
        match mode {
            Mode::G5 => {
                let base = u32::from(self.regs[2] & 0x60) << 10;
                let byte = self.vram_at(base | (y32 * 128 + hx32 / 4), false);
                (byte >> (6 - 2 * (hx & 3))) & 3
            }
            Mode::G6 => {
                let base = u32::from(self.regs[2] & 0x20) << 11;
                let byte = self.vram_at(base | (y32 * 256 + hx32 / 2), true);
                if hx & 1 == 0 { byte >> 4 } else { byte & 0x0F }
            }
            Mode::T2 => self.text2_pixel(hx, y),
            _ => 0,
        }
    }

    /// Text 2: 80 columns of six hi-res pixels, sixteen pixels of background
    /// either side, and a blink attribute per character.
    fn text2_pixel(&self, hx: usize, y: usize) -> u8 {
        let (mut fg, mut bg) = (self.regs[7] >> 4, self.regs[7] & 0x0F);
        if !(16..496).contains(&hx) {
            return bg;
        }
        let column = ((hx - 16) / 6) as u32;
        let bit = (hx - 16) % 6;
        let row = (y / 8) as u32;
        let cell = row * 80 + column;
        let name_base = u32::from(self.regs[2] & 0x7C) << 10;
        let name = self.vram_at(name_base | cell, false);
        let pattern_base = u32::from(self.regs[4] & 0x3F) << 11;
        let pattern = self.vram_at(pattern_base | (u32::from(name) * 8 + (y as u32 & 7)), false);
        if self.blink_on {
            let blink_base =
                (u32::from(self.regs[10]) << 14) | (u32::from(self.regs[3] & 0xF8) << 6);
            let attributes = self.vram_at(blink_base | (cell / 8), false);
            if attributes & (0x80 >> (cell % 8)) != 0 {
                fg = self.regs[12] >> 4;
                bg = self.regs[12] & 0x0F;
            }
        }
        if pattern & (0x80 >> bit) != 0 { fg } else { bg }
    }
}

/// The foreground or background half of a tile colour byte, by pattern bit.
fn pick(pattern: u8, x: usize, colour: u8) -> u8 {
    if pattern & (0x80 >> (x & 7)) != 0 {
        colour >> 4
    } else {
        colour & 0x0F
    }
}

/// A Graphic 7 colour byte, `GGGRRRBB`, as ARGB32. Blue's two bits stretch
/// to three by repeating the high bit.
#[must_use]
pub(crate) fn grb332_to_argb(value: u8) -> u32 {
    let g = u16::from(value >> 5);
    let r = u16::from((value >> 2) & 7);
    let b2 = u16::from(value & 3);
    let b = (b2 << 1) | (b2 >> 1);
    V9938::grb_to_argb((g << 8) | (r << 4) | b)
}
