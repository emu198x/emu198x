//! Sprites, evaluated once per line.
//!
//! **Mode 1** (G1, G2, MC) is the TMS9918's: 32 sprites, four a line, one
//! colour each from the attribute table, `Y = 208` ending the list.
//!
//! **Mode 2** (G3-G7) allows eight a line and takes each sprite's colour
//! per line from a colour table 512 bytes below the attribute table. A
//! colour byte's bits are: 0-3 colour, 5 IC (ignore collisions), 6 CC
//! (combine with the sprite above), 7 EC (shift 32 pixels left). `Y = 216`
//! ends the list.
//!
//! **CC** is modelled as hardware is usually described: a CC sprite's pixel
//! is ORed into a pixel a non-CC sprite earlier in the list already drew,
//! and is not shown where none did. CC sprites take no part in collision.
//!
//! In the planar modes (G6, G7) the tables are read through the planar
//! mapping, as every VRAM read is.

use crate::{Mode, V9938};

impl V9938 {
    fn sprite_byte(&self, logical: u32, planar: bool) -> u8 {
        if planar {
            self.vram[Self::planar(logical)]
        } else {
            self.vram[(logical as usize) & 0x1FFFF]
        }
    }

    /// Fill this line's sprite buffer and update the overflow and
    /// collision status.
    pub(crate) fn prepare_line_sprites(&mut self) {
        self.sprite_line.iter_mut().for_each(|p| *p = 0);
        let mode = self.mode();
        let sprites_on = !matches!(mode, Mode::T1 | Mode::T2 | Mode::Undefined)
            && self.regs[1] & 0x40 != 0
            && self.regs[8] & 0x02 == 0;
        if !sprites_on {
            return;
        }
        let mode2 = mode.sprite_mode_2();
        let planar = mode.is_planar();
        let large = self.regs[1] & 0x02 != 0;
        let magnified = self.regs[1] & 0x01 != 0;
        let size: u16 = if large { 16 } else { 8 };
        let height = if magnified { size * 2 } else { size };
        let y_line = (self.line + u16::from(self.regs[23])) & 0xFF;

        let table = (u32::from(self.regs[11]) << 15) | (u32::from(self.regs[5]) << 7);
        let (attributes, end, limit) = if mode2 {
            ((table & 0x1_FC00) | 0x200, 216u8, 8usize)
        } else {
            (table & 0x1_FF80, 208u8, 4usize)
        };
        let colours = attributes.wrapping_sub(0x200);
        let patterns = u32::from(self.regs[6] & 0x3F) << 11;

        // Find the sprites on this line.
        let mut found: Vec<(u8, u16)> = Vec::with_capacity(limit);
        let mut last = 0u8;
        for index in 0..32u8 {
            let base = attributes + u32::from(index) * 4;
            let y = self.sprite_byte(base, planar);
            if y == end {
                break;
            }
            last = index;
            let row = y_line.wrapping_sub(u16::from(y)).wrapping_sub(1) & 0xFF;
            if row >= height {
                continue;
            }
            if found.len() == limit {
                if self.status0 & 0x40 == 0 {
                    self.status0 = (self.status0 & 0xA0) | 0x40 | index;
                }
                break;
            }
            found.push((index, row));
        }
        if self.status0 & 0x40 == 0 {
            self.status0 = (self.status0 & 0xE0) | last;
        }

        // Draw them, earliest first.
        let mut drawn = [false; 256];
        let mut solid = [false; 256];
        let mut collision: Option<(u16, u16)> = None;
        for &(index, row) in &found {
            let base = attributes + u32::from(index) * 4;
            let x = i32::from(self.sprite_byte(base + 1, planar));
            let mut pattern = u32::from(self.sprite_byte(base + 2, planar));
            if large {
                pattern &= 0xFC;
            }
            let pattern_row = u32::from(if magnified { row / 2 } else { row });
            let (colour, early, combine, ignore) = if mode2 {
                let byte = self.sprite_byte(colours + u32::from(index) * 16 + pattern_row, planar);
                (
                    byte & 0x0F,
                    byte & 0x80 != 0,
                    byte & 0x40 != 0,
                    byte & 0x20 != 0,
                )
            } else {
                let byte = self.sprite_byte(base + 3, planar);
                (byte & 0x0F, byte & 0x80 != 0, false, false)
            };
            let left = self.sprite_byte(patterns + pattern * 8 + pattern_row, planar);
            let right = if large {
                self.sprite_byte(patterns + pattern * 8 + 16 + pattern_row, planar)
            } else {
                0
            };
            let bits = (u16::from(left) << 8) | u16::from(right);
            let start = x - if early { 32 } else { 0 };
            let scale = if magnified { 2 } else { 1 };
            for bit in 0..i32::from(size) {
                if bits & (0x8000 >> bit) == 0 {
                    continue;
                }
                for half in 0..scale {
                    let px = start + bit * scale + half;
                    if !(0..256).contains(&px) {
                        continue;
                    }
                    let px = px as usize;
                    if combine {
                        if drawn[px] {
                            let mixed = (self.sprite_line[px] & 0x0F) | colour;
                            if mixed != 0 || self.regs[8] & 0x20 != 0 {
                                self.sprite_line[px] = 0x80 | mixed;
                            }
                        }
                        continue;
                    }
                    if drawn[px] {
                        if solid[px] && !ignore && collision.is_none() {
                            collision = Some((px as u16, y_line));
                        }
                        continue;
                    }
                    drawn[px] = true;
                    solid[px] = !ignore;
                    let visible = colour != 0 || self.regs[8] & 0x20 != 0;
                    if visible {
                        self.sprite_line[px] = 0x80 | colour;
                    }
                }
            }
        }
        if let Some((x, y)) = collision {
            self.status0 |= 0x20;
            if mode2 && self.collision_x == 0 && self.collision_y == 0 {
                // S#3-S#6 report the position offset as openMSX measured.
                self.collision_x = x + 12;
                self.collision_y = y + 8;
            }
        }
    }
}
