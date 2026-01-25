//! NES Picture Processing Unit (PPU 2C02).
//!
//! The PPU handles all video output:
//! - 256x240 pixel output
//! - 2 pattern tables (CHR ROM/RAM)
//! - 4 nametables (2KB internal, mirrored)
//! - 64 sprites (8 per scanline)
//! - 64-color palette (52 distinct colors)

use crate::memory::NesMemory;

/// PPU control register flags ($2000).
pub mod ctrl {
    pub const NAMETABLE_X: u8 = 0x01;
    pub const NAMETABLE_Y: u8 = 0x02;
    pub const VRAM_INCREMENT: u8 = 0x04; // 0=+1, 1=+32
    pub const SPRITE_PATTERN: u8 = 0x08;
    pub const BG_PATTERN: u8 = 0x10;
    pub const SPRITE_SIZE: u8 = 0x20; // 0=8x8, 1=8x16
    pub const MASTER_SLAVE: u8 = 0x40;
    pub const NMI_ENABLE: u8 = 0x80;
}

/// PPU mask register flags ($2001).
pub mod mask {
    pub const GREYSCALE: u8 = 0x01;
    pub const BG_LEFT: u8 = 0x02;
    pub const SPRITE_LEFT: u8 = 0x04;
    pub const BG_ENABLE: u8 = 0x08;
    pub const SPRITE_ENABLE: u8 = 0x10;
    pub const EMPHASIZE_RED: u8 = 0x20;
    pub const EMPHASIZE_GREEN: u8 = 0x40;
    pub const EMPHASIZE_BLUE: u8 = 0x80;
}

/// PPU status register flags ($2002).
pub mod status {
    pub const SPRITE_OVERFLOW: u8 = 0x20;
    pub const SPRITE_0_HIT: u8 = 0x40;
    pub const VBLANK: u8 = 0x80;
}

/// NES PPU.
pub struct Ppu {
    /// Control register ($2000).
    pub ctrl: u8,
    /// Mask register ($2001).
    pub mask: u8,
    /// Status register ($2002).
    pub status: u8,
    /// OAM address ($2003).
    pub oam_addr: u8,
    /// OAM data (256 bytes, 64 sprites x 4 bytes).
    pub oam: [u8; 256],
    /// Current VRAM address (loopy_v).
    pub vram_addr: u16,
    /// Temporary VRAM address (loopy_t).
    temp_addr: u16,
    /// Fine X scroll (3 bits).
    fine_x: u8,
    /// Write toggle (for $2005/$2006).
    pub write_toggle: bool,
    /// Data buffer for $2007 reads.
    pub data_buffer: u8,
    /// Current scanline (0-261).
    scanline: u16,
    /// Current cycle within scanline (0-340).
    cycle: u16,
    /// Frame is odd (for NTSC skip).
    odd_frame: bool,
    /// NMI occurred this frame.
    pub nmi_occurred: bool,

    // Background rendering shift registers
    /// Background tile shift register (low bits).
    bg_shift_lo: u16,
    /// Background tile shift register (high bits).
    bg_shift_hi: u16,
    /// Attribute shift register (low bits).
    attr_shift_lo: u16,
    /// Attribute shift register (high bits).
    attr_shift_hi: u16,
    /// Attribute latch (low bit).
    attr_latch_lo: bool,
    /// Attribute latch (high bit).
    attr_latch_hi: bool,

    // Background tile fetch latches
    /// Nametable byte.
    nt_byte: u8,
    /// Attribute byte.
    attr_byte: u8,
    /// Pattern table low byte.
    pt_lo: u8,
    /// Pattern table high byte.
    pt_hi: u8,

    /// Frame buffer (256x240, palette indices).
    pub framebuffer: [u8; 256 * 240],

    // Sprite rendering state
    /// Secondary OAM (sprites for current scanline, 8 sprites x 4 bytes).
    secondary_oam: [u8; 32],
    /// Number of sprites on current scanline.
    sprite_count: u8,
    /// Sprite pattern shift registers (low bits).
    sprite_shift_lo: [u8; 8],
    /// Sprite pattern shift registers (high bits).
    sprite_shift_hi: [u8; 8],
    /// Sprite attributes for current scanline.
    sprite_attrs: [u8; 8],
    /// Sprite X positions for current scanline.
    sprite_x: [u8; 8],
    /// Sprite 0 is on this scanline.
    sprite_0_on_line: bool,
    /// Sprite 0 is being rendered.
    sprite_0_rendering: bool,
}

impl Ppu {
    /// Create a new PPU.
    pub fn new() -> Self {
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            oam: [0; 256],
            vram_addr: 0,
            temp_addr: 0,
            fine_x: 0,
            write_toggle: false,
            data_buffer: 0,
            scanline: 0,
            cycle: 0,
            odd_frame: false,
            nmi_occurred: false,
            bg_shift_lo: 0,
            bg_shift_hi: 0,
            attr_shift_lo: 0,
            attr_shift_hi: 0,
            attr_latch_lo: false,
            attr_latch_hi: false,
            nt_byte: 0,
            attr_byte: 0,
            pt_lo: 0,
            pt_hi: 0,
            framebuffer: [0; 256 * 240],
            secondary_oam: [0xFF; 32],
            sprite_count: 0,
            sprite_shift_lo: [0; 8],
            sprite_shift_hi: [0; 8],
            sprite_attrs: [0; 8],
            sprite_x: [0; 8],
            sprite_0_on_line: false,
            sprite_0_rendering: false,
        }
    }

    /// Reset the PPU.
    pub fn reset(&mut self) {
        self.ctrl = 0;
        self.mask = 0;
        self.status = 0;
        self.write_toggle = false;
        self.scanline = 0;
        self.cycle = 0;
        self.odd_frame = false;
        self.nmi_occurred = false;
        self.bg_shift_lo = 0;
        self.bg_shift_hi = 0;
        self.attr_shift_lo = 0;
        self.attr_shift_hi = 0;
        self.secondary_oam = [0xFF; 32];
        self.sprite_count = 0;
        self.sprite_0_on_line = false;
        self.sprite_0_rendering = false;
    }

    /// Read PPU register (memory-mapped $2000-$2007).
    pub fn read_register(&mut self, addr: u16, memory: &NesMemory) -> u8 {
        match addr & 0x07 {
            // $2002 - Status
            2 => {
                let status = self.status;
                self.status &= !status::VBLANK;
                self.write_toggle = false;
                self.nmi_occurred = false;
                status
            }
            // $2004 - OAM data
            4 => self.oam[self.oam_addr as usize],
            // $2007 - VRAM data
            7 => {
                let addr = self.vram_addr & 0x3FFF;
                let data = if addr >= 0x3F00 {
                    // Palette reads are immediate
                    memory.ppu_read(addr)
                } else {
                    // Other reads are buffered
                    let buffered = self.data_buffer;
                    self.data_buffer = memory.ppu_read(addr);
                    buffered
                };
                self.increment_vram_addr();
                data
            }
            _ => 0,
        }
    }

    /// Write PPU register (memory-mapped $2000-$2007).
    pub fn write_register(&mut self, addr: u16, value: u8, memory: &mut NesMemory) {
        match addr & 0x07 {
            // $2000 - Control
            0 => {
                self.ctrl = value;
                // t: ...GH.. ........ <- d: ......GH
                self.temp_addr = (self.temp_addr & 0xF3FF) | ((value as u16 & 0x03) << 10);
            }
            // $2001 - Mask
            1 => self.mask = value,
            // $2003 - OAM address
            3 => self.oam_addr = value,
            // $2004 - OAM data
            4 => {
                self.oam[self.oam_addr as usize] = value;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            // $2005 - Scroll
            5 => {
                if !self.write_toggle {
                    // First write: X scroll
                    self.temp_addr = (self.temp_addr & 0xFFE0) | ((value as u16) >> 3);
                    self.fine_x = value & 0x07;
                } else {
                    // Second write: Y scroll
                    self.temp_addr = (self.temp_addr & 0x8C1F)
                        | ((value as u16 & 0x07) << 12)
                        | ((value as u16 & 0xF8) << 2);
                }
                self.write_toggle = !self.write_toggle;
            }
            // $2006 - VRAM address
            6 => {
                if !self.write_toggle {
                    // First write: high byte
                    self.temp_addr = (self.temp_addr & 0x00FF) | ((value as u16 & 0x3F) << 8);
                } else {
                    // Second write: low byte
                    self.temp_addr = (self.temp_addr & 0xFF00) | (value as u16);
                    self.vram_addr = self.temp_addr;
                }
                self.write_toggle = !self.write_toggle;
            }
            // $2007 - VRAM data
            7 => {
                memory.ppu_write(self.vram_addr & 0x3FFF, value);
                self.increment_vram_addr();
            }
            _ => {}
        }
    }

    /// Increment VRAM address by 1 or 32 based on CTRL register.
    pub fn increment_vram_addr(&mut self) {
        let increment = if self.ctrl & ctrl::VRAM_INCREMENT != 0 {
            32
        } else {
            1
        };
        self.vram_addr = self.vram_addr.wrapping_add(increment);
    }

    /// Tick PPU for one cycle.
    /// Returns (nmi_triggered, pixel_output).
    pub fn tick(&mut self, memory: &mut NesMemory) -> (bool, Option<u8>) {
        let mut nmi = false;

        // Pre-render scanline (261)
        if self.scanline == 261 {
            if self.cycle == 1 {
                // Clear vblank, sprite 0 hit, overflow
                self.status &= !(status::VBLANK | status::SPRITE_0_HIT | status::SPRITE_OVERFLOW);
                self.nmi_occurred = false;
            }

            // Copy vertical bits from t to v at cycle 280-304
            if self.rendering_enabled() && self.cycle >= 280 && self.cycle <= 304 {
                // v: GHIA.BC DEF..... <- t: GHIA.BC DEF.....
                self.vram_addr = (self.vram_addr & 0x041F) | (self.temp_addr & 0x7BE0);
            }

            // Background tile fetches (cycles 321-336)
            if self.rendering_enabled() && self.cycle >= 321 && self.cycle <= 336 {
                self.fetch_background_tile(memory);
            }

            // Skip cycle on odd frames when rendering enabled
            if self.cycle == 339 && self.odd_frame && self.rendering_enabled() {
                self.cycle = 340;
            }
        }

        // Visible scanlines (0-239)
        if self.scanline < 240 {
            // Render pixel during cycles 1-256
            if self.cycle >= 1 && self.cycle <= 256 {
                self.render_pixel(memory);

                // Shift registers
                self.bg_shift_lo <<= 1;
                self.bg_shift_hi <<= 1;
                self.attr_shift_lo <<= 1;
                self.attr_shift_hi <<= 1;

                // Refill attribute shift registers from latches
                if self.attr_latch_lo {
                    self.attr_shift_lo |= 1;
                }
                if self.attr_latch_hi {
                    self.attr_shift_hi |= 1;
                }
            }

            // Background tile fetches
            if self.rendering_enabled() {
                if (self.cycle >= 1 && self.cycle <= 256) || (self.cycle >= 321 && self.cycle <= 336)
                {
                    self.fetch_background_tile(memory);
                }

                // Increment coarse X at cycle 256
                if self.cycle == 256 {
                    self.increment_y();
                }

                // Copy horizontal bits from t to v at cycle 257
                if self.cycle == 257 {
                    // v: ....A.. ...BCDEF <- t: ....A.. ...BCDEF
                    self.vram_addr = (self.vram_addr & 0x7BE0) | (self.temp_addr & 0x041F);

                    // Evaluate sprites for next scanline
                    self.evaluate_sprites();
                }

                // Fetch sprite data during cycles 257-320
                if self.cycle >= 257 && self.cycle <= 320 {
                    self.fetch_sprite_data(memory);
                }
            }
        }

        // Vblank start (scanline 241)
        if self.scanline == 241 && self.cycle == 1 {
            self.status |= status::VBLANK;
            if self.ctrl & ctrl::NMI_ENABLE != 0 && !self.nmi_occurred {
                nmi = true;
                self.nmi_occurred = true;
            }
        }

        // Advance cycle/scanline
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.odd_frame = !self.odd_frame;
            }
        }

        (nmi, None)
    }

    /// Fetch background tile data based on current cycle.
    fn fetch_background_tile(&mut self, memory: &NesMemory) {
        let cycle_in_tile = (self.cycle - 1) & 0x07;

        match cycle_in_tile {
            0 => {
                // Load shift registers with new tile data
                self.load_background_shifters();
            }
            1 => {
                // Fetch nametable byte
                let addr = 0x2000 | (self.vram_addr & 0x0FFF);
                self.nt_byte = memory.ppu_read(addr);
            }
            3 => {
                // Fetch attribute byte
                let v = self.vram_addr;
                let addr = 0x23C0
                    | (v & 0x0C00)
                    | ((v >> 4) & 0x38)
                    | ((v >> 2) & 0x07);
                let attr = memory.ppu_read(addr);

                // Select 2-bit palette from attribute byte
                let shift = ((v >> 4) & 0x04) | (v & 0x02);
                self.attr_byte = (attr >> shift) & 0x03;
            }
            5 => {
                // Fetch pattern table low byte
                let pattern_base = if self.ctrl & ctrl::BG_PATTERN != 0 {
                    0x1000
                } else {
                    0x0000
                };
                let fine_y = (self.vram_addr >> 12) & 0x07;
                let addr = pattern_base + (self.nt_byte as u16) * 16 + fine_y;
                self.pt_lo = memory.ppu_read(addr);
            }
            7 => {
                // Fetch pattern table high byte
                let pattern_base = if self.ctrl & ctrl::BG_PATTERN != 0 {
                    0x1000
                } else {
                    0x0000
                };
                let fine_y = (self.vram_addr >> 12) & 0x07;
                let addr = pattern_base + (self.nt_byte as u16) * 16 + fine_y + 8;
                self.pt_hi = memory.ppu_read(addr);

                // Increment coarse X
                if self.rendering_enabled() {
                    self.increment_x();
                }
            }
            _ => {}
        }
    }

    /// Load fetched tile data into shift registers.
    fn load_background_shifters(&mut self) {
        // Load pattern bytes into low 8 bits of shift registers
        self.bg_shift_lo = (self.bg_shift_lo & 0xFF00) | (self.pt_lo as u16);
        self.bg_shift_hi = (self.bg_shift_hi & 0xFF00) | (self.pt_hi as u16);

        // Load attribute bits into latches (expanded to fill all 8 bits)
        self.attr_latch_lo = self.attr_byte & 0x01 != 0;
        self.attr_latch_hi = self.attr_byte & 0x02 != 0;
    }

    /// Render a single pixel to the framebuffer.
    fn render_pixel(&mut self, memory: &NesMemory) {
        let x = self.cycle - 1;
        let y = self.scanline;

        if x >= 256 || y >= 240 {
            return;
        }

        let mut bg_pixel = 0u8;
        let mut bg_palette = 0u8;

        // Get background pixel
        if self.mask & mask::BG_ENABLE != 0 {
            // Skip leftmost 8 pixels if BG_LEFT is not set
            if x >= 8 || self.mask & mask::BG_LEFT != 0 {
                let shift = 15 - self.fine_x;
                let lo = ((self.bg_shift_lo >> shift) & 1) as u8;
                let hi = ((self.bg_shift_hi >> shift) & 1) as u8;
                bg_pixel = (hi << 1) | lo;

                let attr_lo = ((self.attr_shift_lo >> shift) & 1) as u8;
                let attr_hi = ((self.attr_shift_hi >> shift) & 1) as u8;
                bg_palette = (attr_hi << 1) | attr_lo;
            }
        }

        // Get sprite pixel
        let mut sprite_pixel = 0u8;
        let mut sprite_palette = 0u8;
        let mut sprite_priority = false;
        let mut sprite_0_visible = false;

        if self.mask & mask::SPRITE_ENABLE != 0 {
            // Skip leftmost 8 pixels if SPRITE_LEFT is not set
            if x >= 8 || self.mask & mask::SPRITE_LEFT != 0 {
                for i in 0..self.sprite_count as usize {
                    let sprite_x = self.sprite_x[i] as u16;

                    // Check if sprite is visible at this pixel
                    if x >= sprite_x && x < sprite_x + 8 {
                        let offset = (x - sprite_x) as u8;
                        let lo = (self.sprite_shift_lo[i] >> (7 - offset)) & 1;
                        let hi = (self.sprite_shift_hi[i] >> (7 - offset)) & 1;
                        let pixel = (hi << 1) | lo;

                        if pixel != 0 {
                            // First non-transparent sprite pixel wins
                            sprite_pixel = pixel;
                            sprite_palette = (self.sprite_attrs[i] & 0x03) + 4; // Sprite palettes are 4-7
                            sprite_priority = self.sprite_attrs[i] & 0x20 != 0; // Behind background

                            // Check for sprite 0 hit
                            if i == 0 && self.sprite_0_rendering {
                                sprite_0_visible = true;
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Sprite 0 hit detection
        if sprite_0_visible && bg_pixel != 0 && x < 255 {
            self.status |= status::SPRITE_0_HIT;
        }

        // Determine final color based on priority
        let (final_pixel, final_palette) = if sprite_pixel == 0 {
            // No sprite, use background
            (bg_pixel, bg_palette)
        } else if bg_pixel == 0 {
            // Background transparent, use sprite
            (sprite_pixel, sprite_palette)
        } else if sprite_priority {
            // Sprite has priority behind background
            (bg_pixel, bg_palette)
        } else {
            // Sprite in front of background
            (sprite_pixel, sprite_palette)
        };

        // Look up color from palette
        let color = if final_pixel == 0 {
            // Transparent - use backdrop color
            memory.ppu_read(0x3F00)
        } else {
            let addr = 0x3F00 + (final_palette as u16) * 4 + (final_pixel as u16);
            memory.ppu_read(addr)
        };

        // Apply greyscale if enabled
        let color = if self.mask & mask::GREYSCALE != 0 {
            color & 0x30
        } else {
            color
        };

        self.framebuffer[(y as usize) * 256 + (x as usize)] = color;
    }

    /// Evaluate which sprites are on the next scanline.
    fn evaluate_sprites(&mut self) {
        // Clear secondary OAM
        self.secondary_oam.fill(0xFF);
        self.sprite_count = 0;
        self.sprite_0_on_line = false;

        let sprite_height = if self.ctrl & ctrl::SPRITE_SIZE != 0 { 16 } else { 8 };
        let next_line = self.scanline.wrapping_add(1) as i16;

        for i in 0..64 {
            let y = self.oam[i * 4] as i16;
            let diff = next_line - y;

            // Check if sprite is on the next scanline
            if diff >= 0 && diff < sprite_height {
                if self.sprite_count < 8 {
                    // Copy sprite to secondary OAM
                    let dest = (self.sprite_count as usize) * 4;
                    self.secondary_oam[dest] = self.oam[i * 4];
                    self.secondary_oam[dest + 1] = self.oam[i * 4 + 1];
                    self.secondary_oam[dest + 2] = self.oam[i * 4 + 2];
                    self.secondary_oam[dest + 3] = self.oam[i * 4 + 3];

                    if i == 0 {
                        self.sprite_0_on_line = true;
                    }

                    self.sprite_count += 1;
                } else {
                    // More than 8 sprites - set overflow flag
                    self.status |= status::SPRITE_OVERFLOW;
                    break;
                }
            }
        }

        self.sprite_0_rendering = self.sprite_0_on_line;
    }

    /// Fetch sprite pattern data.
    fn fetch_sprite_data(&mut self, memory: &NesMemory) {
        // Sprite fetches happen during cycles 257-320 (8 sprites, 8 cycles each)
        let sprite_index = ((self.cycle - 257) / 8) as usize;

        if sprite_index >= 8 {
            return;
        }

        let cycle_in_sprite = (self.cycle - 257) % 8;

        // Only fetch on specific cycles within each sprite's 8-cycle window
        if cycle_in_sprite != 5 && cycle_in_sprite != 7 {
            return;
        }

        // Get sprite data from secondary OAM
        let y = self.secondary_oam[sprite_index * 4] as u16;
        let tile = self.secondary_oam[sprite_index * 4 + 1];
        let attr = self.secondary_oam[sprite_index * 4 + 2];
        let x = self.secondary_oam[sprite_index * 4 + 3];

        self.sprite_attrs[sprite_index] = attr;
        self.sprite_x[sprite_index] = x;

        // Skip if sprite is not visible (Y >= 0xEF)
        if y >= 0xEF {
            self.sprite_shift_lo[sprite_index] = 0;
            self.sprite_shift_hi[sprite_index] = 0;
            return;
        }

        let sprite_height = if self.ctrl & ctrl::SPRITE_SIZE != 0 { 16 } else { 8 };
        let next_line = self.scanline.wrapping_add(1) as u16;
        let mut row = next_line.wrapping_sub(y) as u8;

        // Vertical flip
        if attr & 0x80 != 0 {
            row = (sprite_height - 1) as u8 - row;
        }

        // Calculate pattern address
        let addr = if self.ctrl & ctrl::SPRITE_SIZE != 0 {
            // 8x16 sprites: tile bit 0 selects pattern table
            let bank = (tile & 0x01) as u16 * 0x1000;
            let tile_num = (tile & 0xFE) as u16;
            if row >= 8 {
                bank + (tile_num + 1) * 16 + ((row - 8) as u16)
            } else {
                bank + tile_num * 16 + (row as u16)
            }
        } else {
            // 8x8 sprites
            let pattern_base = if self.ctrl & ctrl::SPRITE_PATTERN != 0 {
                0x1000
            } else {
                0x0000
            };
            pattern_base + (tile as u16) * 16 + (row as u16)
        };

        // Fetch pattern data on cycles 5 and 7
        if cycle_in_sprite == 5 {
            let mut lo = memory.ppu_read(addr);

            // Horizontal flip
            if attr & 0x40 != 0 {
                lo = lo.reverse_bits();
            }

            self.sprite_shift_lo[sprite_index] = lo;
        } else if cycle_in_sprite == 7 {
            let mut hi = memory.ppu_read(addr + 8);

            // Horizontal flip
            if attr & 0x40 != 0 {
                hi = hi.reverse_bits();
            }

            self.sprite_shift_hi[sprite_index] = hi;
        }
    }

    /// Increment coarse X in VRAM address.
    fn increment_x(&mut self) {
        if (self.vram_addr & 0x001F) == 31 {
            // Wrap around and switch horizontal nametable
            self.vram_addr &= !0x001F;
            self.vram_addr ^= 0x0400;
        } else {
            self.vram_addr += 1;
        }
    }

    /// Increment Y in VRAM address.
    fn increment_y(&mut self) {
        if (self.vram_addr & 0x7000) != 0x7000 {
            // Increment fine Y
            self.vram_addr += 0x1000;
        } else {
            // Reset fine Y and increment coarse Y
            self.vram_addr &= !0x7000;
            let mut coarse_y = (self.vram_addr & 0x03E0) >> 5;

            if coarse_y == 29 {
                // Row 29 is last row of tiles, switch vertical nametable
                coarse_y = 0;
                self.vram_addr ^= 0x0800;
            } else if coarse_y == 31 {
                // Coarse Y wraps without switching nametable
                coarse_y = 0;
            } else {
                coarse_y += 1;
            }

            self.vram_addr = (self.vram_addr & !0x03E0) | (coarse_y << 5);
        }
    }

    /// Check if rendering is enabled.
    pub fn rendering_enabled(&self) -> bool {
        self.mask & (mask::BG_ENABLE | mask::SPRITE_ENABLE) != 0
    }

    /// Get current scanline.
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Get current cycle.
    pub fn cycle(&self) -> u16 {
        self.cycle
    }

    /// Get VRAM address (for debugging).
    pub fn vram_addr(&self) -> u16 {
        self.vram_addr
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ppu() {
        let ppu = Ppu::new();
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.cycle(), 0);
    }

    #[test]
    fn test_vram_increment() {
        let mut ppu = Ppu::new();

        // Increment by 1 (default)
        ppu.vram_addr = 0x2000;
        ppu.ctrl = 0;
        ppu.increment_vram_addr();
        assert_eq!(ppu.vram_addr, 0x2001);

        // Increment by 32
        ppu.vram_addr = 0x2000;
        ppu.ctrl = ctrl::VRAM_INCREMENT;
        ppu.increment_vram_addr();
        assert_eq!(ppu.vram_addr, 0x2020);
    }

    #[test]
    fn test_increment_x() {
        let mut ppu = Ppu::new();

        // Normal increment
        ppu.vram_addr = 0x2000;
        ppu.increment_x();
        assert_eq!(ppu.vram_addr & 0x001F, 1);

        // Wrap and switch nametable
        ppu.vram_addr = 0x2000 | 31;
        ppu.increment_x();
        assert_eq!(ppu.vram_addr & 0x001F, 0);
        assert!(ppu.vram_addr & 0x0400 != 0); // Horizontal nametable bit flipped
    }

    #[test]
    fn test_increment_y() {
        let mut ppu = Ppu::new();

        // Increment fine Y from 0 to 1
        ppu.vram_addr = 0x0000; // fine_y = 0
        ppu.increment_y();
        assert_eq!(ppu.vram_addr & 0x7000, 0x1000); // fine_y = 1

        // Fine Y wraps (7 -> 0), increment coarse Y
        ppu.vram_addr = 0x7000; // fine_y = 7, coarse_y = 0
        ppu.increment_y();
        assert_eq!(ppu.vram_addr & 0x7000, 0); // fine_y = 0
        assert_eq!((ppu.vram_addr & 0x03E0) >> 5, 1); // coarse_y = 1
    }
}
