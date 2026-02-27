//! NES PPU (2C02) emulation.
//!
//! Dot-based rendering. One `tick()` = one PPU dot. The PPU runs at
//! 5,369,318 Hz (21,477,272 / 4). Each frame is 341 dots x 262 scanlines.
//!
//! ## Scanline layout
//! - 0-239: visible scanlines (render pixels)
//! - 240: post-render (idle)
//! - 241-260: `VBlank`
//! - 261: pre-render

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unused_self
)]
#![allow(
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::manual_range_contains
)]

use crate::cartridge::{Mapper, Mirroring};
use crate::palette::PALETTE;

/// Framebuffer dimensions.
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 240;

/// PPU 2C02.
pub struct Ppu {
    // VRAM
    nametable_ram: [u8; 2048],
    palette_ram: [u8; 32],
    oam: [u8; 256],

    // Registers
    ctrl: u8,
    mask: u8,
    status: u8,
    oam_addr: u8,

    // Loopy scroll/address registers
    v: u16,
    t: u16,
    fine_x: u8,
    w: bool,

    // Data read buffer ($2007)
    read_buffer: u8,

    // Rendering position
    scanline: u16,
    dot: u16,
    frame_odd: bool,

    // Background shift registers
    bg_shift_pattern_lo: u16,
    bg_shift_pattern_hi: u16,
    bg_shift_attrib_lo: u16,
    bg_shift_attrib_hi: u16,
    bg_next_tile_id: u8,
    bg_next_tile_attrib: u8,
    bg_next_tile_lo: u8,
    bg_next_tile_hi: u8,

    // Sprite evaluation
    secondary_oam: [u8; 32],
    sprite_count: u8,
    sprite_patterns_lo: [u8; 8],
    sprite_patterns_hi: [u8; 8],
    sprite_attribs: [u8; 8],
    sprite_x_counters: [u8; 8],
    sprite_zero_on_line: bool,

    // Output
    framebuffer: Vec<u32>,
    nmi_occurred: bool,
    nmi_output: bool,
    nmi_edge: bool,
}

impl Ppu {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nametable_ram: [0; 2048],
            palette_ram: [0; 32],
            oam: [0; 256],

            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,

            v: 0,
            t: 0,
            fine_x: 0,
            w: false,

            read_buffer: 0,

            scanline: 261, // Start at pre-render
            dot: 0,
            frame_odd: false,

            bg_shift_pattern_lo: 0,
            bg_shift_pattern_hi: 0,
            bg_shift_attrib_lo: 0,
            bg_shift_attrib_hi: 0,
            bg_next_tile_id: 0,
            bg_next_tile_attrib: 0,
            bg_next_tile_lo: 0,
            bg_next_tile_hi: 0,

            secondary_oam: [0xFF; 32],
            sprite_count: 0,
            sprite_patterns_lo: [0; 8],
            sprite_patterns_hi: [0; 8],
            sprite_attribs: [0; 8],
            sprite_x_counters: [0; 8],
            sprite_zero_on_line: false,

            framebuffer: vec![0; (FB_WIDTH * FB_HEIGHT) as usize],
            nmi_occurred: false,
            nmi_output: false,
            nmi_edge: false,
        }
    }

    /// One PPU dot.
    pub fn tick(&mut self, mapper: &mut dyn Mapper) {
        // Pre-render line (261)
        if self.scanline == 261 {
            self.tick_prerender(mapper);
        }
        // Visible scanlines (0-239)
        else if self.scanline <= 239 {
            self.tick_visible(mapper);
        }
        // Post-render (240): idle
        // VBlank start (241)
        else if self.scanline == 241 && self.dot == 1 {
            self.status |= 0x80; // Set VBlank flag
            self.nmi_occurred = true;
            self.check_nmi();
        }

        // Advance dot/scanline
        self.dot += 1;
        if self.dot > 340 {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.frame_odd = !self.frame_odd;
            }
        }
    }

    fn tick_prerender(&mut self, mapper: &mut dyn Mapper) {
        if self.dot == 1 {
            // Clear VBlank, sprite 0 hit, sprite overflow
            self.status &= 0x1F;
            self.nmi_occurred = false;
            // Clear sprite shift registers
            self.sprite_patterns_lo = [0; 8];
            self.sprite_patterns_hi = [0; 8];
        }

        if self.rendering_enabled() {
            // Background fetches (same timing as visible lines)
            if (self.dot >= 1 && self.dot <= 256) || (self.dot >= 321 && self.dot <= 336) {
                self.bg_fetch_cycle(mapper);
                self.shift_registers();
            }

            if self.dot == 256 {
                self.increment_y();
            }
            if self.dot == 257 {
                self.copy_horizontal();
            }

            // Copy vertical bits from t to v during dots 280-304
            if self.dot >= 280 && self.dot <= 304 {
                self.copy_vertical();
            }

            // Odd frame skip: skip last dot on odd frames
            if self.dot == 339 && self.frame_odd {
                self.dot = 340; // Will wrap to 0 on next advance
            }
        }
    }

    fn tick_visible(&mut self, mapper: &mut dyn Mapper) {
        if self.rendering_enabled() {
            // Pixel output (dots 1-256)
            if self.dot >= 1 && self.dot <= 256 {
                self.render_pixel(mapper);
                self.bg_fetch_cycle(mapper);
                self.shift_registers();
            }

            // Sprite evaluation at dot 257
            if self.dot == 257 {
                self.evaluate_sprites(mapper);
            }

            // Sprite tile fetches (dots 257-320) — handled in evaluate_sprites

            // Prefetch next scanline tiles (dots 321-336)
            if self.dot >= 321 && self.dot <= 336 {
                self.bg_fetch_cycle(mapper);
                self.shift_registers();
            }

            if self.dot == 256 {
                self.increment_y();
            }
            if self.dot == 257 {
                self.copy_horizontal();
            }
        } else if self.dot >= 1 && self.dot <= 256 {
            // Rendering disabled: output background colour
            let bg_colour = self.palette_ram[0] & 0x3F;
            let x = (self.dot - 1) as usize;
            let y = self.scanline as usize;
            if y < FB_HEIGHT as usize && x < FB_WIDTH as usize {
                self.framebuffer[y * FB_WIDTH as usize + x] = self.apply_mask_effects(bg_colour);
            }
        }
    }

    fn bg_fetch_cycle(&mut self, mapper: &mut dyn Mapper) {
        let cycle = if self.dot >= 321 {
            self.dot - 321
        } else {
            self.dot - 1
        };

        match cycle & 0x07 {
            0 => {
                // Load shift registers with previously fetched tile data
                // (every 8 dots except the very first fetch at dot 321)
                if self.dot != 321 {
                    self.load_bg_shift_registers();
                }
                // Fetch nametable byte
                let nt_addr = 0x2000 | (self.v & 0x0FFF);
                self.bg_next_tile_id = self.ppu_read(nt_addr, mapper);
            }
            2 => {
                // Fetch attribute byte
                let attr_addr =
                    0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07);
                let attr_byte = self.ppu_read(attr_addr, mapper);
                // Select the 2-bit palette for this quadrant
                let shift = ((self.v >> 4) & 0x04) | (self.v & 0x02);
                self.bg_next_tile_attrib = (attr_byte >> shift) & 0x03;
            }
            4 => {
                // Fetch pattern table low byte
                let bg_table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0 };
                let fine_y = (self.v >> 12) & 0x07;
                let addr = bg_table + u16::from(self.bg_next_tile_id) * 16 + fine_y;
                self.bg_next_tile_lo = self.ppu_read(addr, mapper);
            }
            6 => {
                // Fetch pattern table high byte
                let bg_table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0 };
                let fine_y = (self.v >> 12) & 0x07;
                let addr = bg_table + u16::from(self.bg_next_tile_id) * 16 + fine_y + 8;
                self.bg_next_tile_hi = self.ppu_read(addr, mapper);
            }
            7 => {
                // Increment coarse X
                self.increment_x();
            }
            _ => {}
        }
    }

    fn load_bg_shift_registers(&mut self) {
        self.bg_shift_pattern_lo =
            (self.bg_shift_pattern_lo & 0xFF00) | u16::from(self.bg_next_tile_lo);
        self.bg_shift_pattern_hi =
            (self.bg_shift_pattern_hi & 0xFF00) | u16::from(self.bg_next_tile_hi);

        let attrib_lo = if self.bg_next_tile_attrib & 0x01 != 0 {
            0xFF
        } else {
            0x00
        };
        let attrib_hi = if self.bg_next_tile_attrib & 0x02 != 0 {
            0xFF
        } else {
            0x00
        };
        self.bg_shift_attrib_lo = (self.bg_shift_attrib_lo & 0xFF00) | attrib_lo;
        self.bg_shift_attrib_hi = (self.bg_shift_attrib_hi & 0xFF00) | attrib_hi;
    }

    fn shift_registers(&mut self) {
        self.bg_shift_pattern_lo <<= 1;
        self.bg_shift_pattern_hi <<= 1;
        self.bg_shift_attrib_lo <<= 1;
        self.bg_shift_attrib_hi <<= 1;
    }

    fn render_pixel(&mut self, _mapper: &mut dyn Mapper) {
        let x = (self.dot - 1) as usize;
        let y = self.scanline as usize;

        if y >= FB_HEIGHT as usize || x >= FB_WIDTH as usize {
            return;
        }

        // Background pixel
        let (bg_pixel, bg_palette) = self.get_bg_pixel();

        // Sprite pixel
        let (sp_pixel, sp_palette, sp_priority, sp_is_zero) = self.get_sprite_pixel(x);

        // Compose final pixel
        let (pixel, palette) = match (bg_pixel, sp_pixel) {
            (0, 0) => (0, 0),
            (0, _) => (sp_pixel, sp_palette),
            (_, 0) => (bg_pixel, bg_palette),
            (_, _) => {
                // Sprite 0 hit detection
                if sp_is_zero && x != 255 && self.bg_and_sprites_enabled() {
                    self.status |= 0x40;
                }
                if sp_priority {
                    (bg_pixel, bg_palette)
                } else {
                    (sp_pixel, sp_palette)
                }
            }
        };

        let colour_addr = if pixel == 0 {
            0
        } else {
            (u16::from(palette) << 2) | u16::from(pixel)
        };
        let palette_index = self.palette_ram[(colour_addr as usize) & 0x1F] & 0x3F;
        self.framebuffer[y * FB_WIDTH as usize + x] = self.apply_mask_effects(palette_index);
    }

    fn get_bg_pixel(&self) -> (u8, u8) {
        if self.mask & 0x08 == 0 {
            return (0, 0);
        }
        // Left 8 pixels clipping
        if self.dot <= 8 && self.mask & 0x02 == 0 {
            return (0, 0);
        }

        let bit_select = 0x8000 >> self.fine_x;
        let pixel_lo = u8::from(self.bg_shift_pattern_lo & bit_select != 0);
        let pixel_hi = u8::from(self.bg_shift_pattern_hi & bit_select != 0);
        let pixel = (pixel_hi << 1) | pixel_lo;

        let palette_lo = u8::from(self.bg_shift_attrib_lo & bit_select != 0);
        let palette_hi = u8::from(self.bg_shift_attrib_hi & bit_select != 0);
        let palette = (palette_hi << 1) | palette_lo;

        (pixel, palette)
    }

    fn get_sprite_pixel(&self, x: usize) -> (u8, u8, bool, bool) {
        if self.mask & 0x10 == 0 {
            return (0, 0, false, false);
        }
        // Left 8 pixels clipping
        if x < 8 && self.mask & 0x04 == 0 {
            return (0, 0, false, false);
        }

        for i in 0..self.sprite_count as usize {
            let offset = x as i16 - i16::from(self.sprite_x_counters[i]);
            if offset < 0 || offset > 7 {
                continue;
            }
            let offset = offset as u8;

            let lo = (self.sprite_patterns_lo[i] >> (7 - offset)) & 1;
            let hi = (self.sprite_patterns_hi[i] >> (7 - offset)) & 1;
            let pixel = (hi << 1) | lo;

            if pixel == 0 {
                continue;
            }

            let palette = (self.sprite_attribs[i] & 0x03) + 4; // Sprite palettes 4-7
            let behind_bg = self.sprite_attribs[i] & 0x20 != 0;
            let is_sprite_zero = self.sprite_zero_on_line && i == 0;

            return (pixel, palette, behind_bg, is_sprite_zero);
        }

        (0, 0, false, false)
    }

    fn evaluate_sprites(&mut self, mapper: &mut dyn Mapper) {
        let sprite_height: u16 = if self.ctrl & 0x20 != 0 { 16 } else { 8 };
        let next_scanline = self.scanline;

        self.secondary_oam = [0xFF; 32];
        self.sprite_count = 0;
        self.sprite_zero_on_line = false;

        for i in 0..64u8 {
            let y = self.oam[i as usize * 4] as u16;
            let diff = next_scanline.wrapping_sub(y);

            if diff < sprite_height {
                if self.sprite_count < 8 {
                    let idx = self.sprite_count as usize;
                    self.secondary_oam[idx * 4] = self.oam[i as usize * 4];
                    self.secondary_oam[idx * 4 + 1] = self.oam[i as usize * 4 + 1];
                    self.secondary_oam[idx * 4 + 2] = self.oam[i as usize * 4 + 2];
                    self.secondary_oam[idx * 4 + 3] = self.oam[i as usize * 4 + 3];

                    if i == 0 {
                        self.sprite_zero_on_line = true;
                    }

                    self.sprite_count += 1;
                } else {
                    // 2C02 hardware bug: after finding 8 sprites, the PPU
                    // continues scanning but increments the OAM byte offset
                    // (m) alongside the sprite index (n) on each miss. This
                    // causes it to compare tile, attribute, or X bytes as
                    // if they were Y coordinates — missing real overflows
                    // and producing false positives.
                    let mut n = (i + 1) as usize;
                    let mut m: usize = 0;
                    while n < 64 {
                        let byte = self.oam[(n * 4 + m) & 0xFF] as u16;
                        if next_scanline.wrapping_sub(byte) < sprite_height {
                            self.status |= 0x20;
                            break;
                        }
                        n += 1;
                        m = (m + 1) & 3;
                    }
                    break;
                }
            }
        }

        // Fetch sprite patterns
        for i in 0..8usize {
            if i < self.sprite_count as usize {
                let sprite_y = self.secondary_oam[i * 4] as u16;
                let tile_index = self.secondary_oam[i * 4 + 1];
                let attribs = self.secondary_oam[i * 4 + 2];
                let sprite_x = self.secondary_oam[i * 4 + 3];

                let flip_v = attribs & 0x80 != 0;
                let mut row = next_scanline.wrapping_sub(sprite_y);

                let (table, tile, sprite_row) = if sprite_height == 16 {
                    // 8x16 sprites: bit 0 of tile = pattern table, bits 1-7 = tile
                    let table = u16::from(tile_index & 1) * 0x1000;
                    let tile = tile_index & 0xFE;
                    if flip_v {
                        row = 15 - row;
                    }
                    if row >= 8 {
                        (table, tile + 1, row - 8)
                    } else {
                        (table, tile, row)
                    }
                } else {
                    // 8x8 sprites
                    let table = if self.ctrl & 0x08 != 0 { 0x1000u16 } else { 0 };
                    if flip_v {
                        row = 7 - row;
                    }
                    (table, tile_index, row)
                };

                let addr = table + u16::from(tile) * 16 + sprite_row;
                let mut lo = self.ppu_read(addr, mapper);
                let mut hi = self.ppu_read(addr + 8, mapper);

                // Horizontal flip
                if attribs & 0x40 != 0 {
                    lo = flip_byte(lo);
                    hi = flip_byte(hi);
                }

                self.sprite_patterns_lo[i] = lo;
                self.sprite_patterns_hi[i] = hi;
                self.sprite_attribs[i] = attribs;
                self.sprite_x_counters[i] = sprite_x;
            } else {
                self.sprite_patterns_lo[i] = 0;
                self.sprite_patterns_hi[i] = 0;
            }
        }
    }

    // === Scrolling ===

    fn increment_x(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400; // Switch horizontal nametable
        } else {
            self.v += 1;
        }
    }

    fn increment_y(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        if (self.v & 0x7000) != 0x7000 {
            self.v += 0x1000; // Increment fine Y
        } else {
            self.v &= !0x7000; // Fine Y = 0
            let mut coarse_y = (self.v & 0x03E0) >> 5;
            if coarse_y == 29 {
                coarse_y = 0;
                self.v ^= 0x0800; // Switch vertical nametable
            } else if coarse_y == 31 {
                coarse_y = 0; // No nametable switch
            } else {
                coarse_y += 1;
            }
            self.v = (self.v & !0x03E0) | (coarse_y << 5);
        }
    }

    fn copy_horizontal(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        // v: ....A .....EDCBA = t: ....A .....EDCBA
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }

    fn copy_vertical(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        // v: GHIA.BC DEF..... = t: GHIA.BC DEF.....
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    // === Register access (CPU side) ===

    /// CPU read from PPU register ($2000-$2007 mirrored).
    pub fn cpu_read(&mut self, reg: u16, mapper: &mut dyn Mapper) -> u8 {
        match reg & 0x07 {
            // $2002 - PPUSTATUS
            2 => {
                let result = (self.status & 0xE0) | (self.read_buffer & 0x1F);
                self.status &= !0x80; // Clear VBlank
                self.nmi_occurred = false;
                self.check_nmi();
                self.w = false; // Reset write toggle
                result
            }
            // $2004 - OAMDATA
            4 => self.oam[self.oam_addr as usize],
            // $2007 - PPUDATA
            7 => {
                let addr = self.v & 0x3FFF;
                let mut result = self.read_buffer;
                self.read_buffer = self.ppu_read(addr, mapper);
                // Palette reads are not buffered
                if addr >= 0x3F00 {
                    result = self.palette_ram[self.mirror_palette_addr(addr) as usize];
                    // Buffer gets the nametable byte "underneath"
                    self.read_buffer = self.ppu_read(addr & 0x2FFF, mapper);
                }
                // Increment v
                self.v = self
                    .v
                    .wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 });
                self.v &= 0x7FFF;
                result
            }
            _ => 0, // Write-only registers
        }
    }

    /// CPU write to PPU register ($2000-$2007 mirrored).
    pub fn cpu_write(&mut self, reg: u16, val: u8, mapper: &mut dyn Mapper) {
        match reg & 0x07 {
            // $2000 - PPUCTRL
            0 => {
                self.ctrl = val;
                // Nametable select bits go to t bits 10-11
                self.t = (self.t & !0x0C00) | (u16::from(val & 0x03) << 10);
                self.nmi_output = val & 0x80 != 0;
                self.check_nmi();
            }
            // $2001 - PPUMASK
            1 => self.mask = val,
            // $2003 - OAMADDR
            3 => self.oam_addr = val,
            // $2004 - OAMDATA
            4 => {
                self.oam[self.oam_addr as usize] = val;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            // $2005 - PPUSCROLL
            5 => {
                if !self.w {
                    // First write: X scroll
                    self.t = (self.t & !0x001F) | (u16::from(val) >> 3);
                    self.fine_x = val & 0x07;
                } else {
                    // Second write: Y scroll
                    self.t = (self.t & !0x73E0)
                        | (u16::from(val & 0x07) << 12)
                        | (u16::from(val >> 3) << 5);
                }
                self.w = !self.w;
            }
            // $2006 - PPUADDR
            6 => {
                if !self.w {
                    // First write: high byte
                    self.t = (self.t & 0x00FF) | (u16::from(val & 0x3F) << 8);
                } else {
                    // Second write: low byte, copy t to v
                    self.t = (self.t & 0xFF00) | u16::from(val);
                    self.v = self.t;
                }
                self.w = !self.w;
            }
            // $2007 - PPUDATA
            7 => {
                let addr = self.v & 0x3FFF;
                self.ppu_write(addr, val, mapper);
                self.v = self
                    .v
                    .wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 });
                self.v &= 0x7FFF;
            }
            _ => {}
        }
    }

    // === PPU memory access ===

    fn ppu_read(&self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => mapper.chr_read(addr),
            0x2000..=0x3EFF => {
                let mirrored = self.mirror_nametable_addr(addr, mapper.mirroring());
                self.nametable_ram[mirrored as usize]
            }
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_addr(addr);
                self.palette_ram[palette_addr as usize]
            }
            _ => 0,
        }
    }

    fn ppu_write(&mut self, addr: u16, val: u8, mapper: &mut dyn Mapper) {
        let addr = addr & 0x3FFF;
        match addr {
            0x0000..=0x1FFF => mapper.chr_write(addr, val),
            0x2000..=0x3EFF => {
                let mirrored = self.mirror_nametable_addr(addr, mapper.mirroring());
                self.nametable_ram[mirrored as usize] = val;
            }
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_addr(addr);
                self.palette_ram[palette_addr as usize] = val;
            }
            _ => {}
        }
    }

    fn mirror_nametable_addr(&self, addr: u16, mirroring: Mirroring) -> u16 {
        let nt_addr = (addr - 0x2000) & 0x0FFF; // 0x000-0xFFF
        match mirroring {
            Mirroring::Horizontal => {
                // Nametables 0,1 → page 0; 2,3 → page 1
                let page = (nt_addr / 0x0800) * 0x0400;
                page + (nt_addr & 0x03FF)
            }
            Mirroring::Vertical => {
                // Nametables 0,2 → page 0; 1,3 → page 1
                nt_addr & 0x07FF
            }
            Mirroring::FourScreen => nt_addr & 0x0FFF,
            Mirroring::SingleScreenLower => {
                // All nametables → page 0
                nt_addr & 0x03FF
            }
            Mirroring::SingleScreenUpper => {
                // All nametables → page 1
                0x0400 + (nt_addr & 0x03FF)
            }
        }
    }

    fn mirror_palette_addr(&self, addr: u16) -> u16 {
        let mut a = (addr - 0x3F00) & 0x1F;
        // Mirror $3F10/$3F14/$3F18/$3F1C to $3F00/$3F04/$3F08/$3F0C
        if a == 0x10 || a == 0x14 || a == 0x18 || a == 0x1C {
            a -= 0x10;
        }
        a
    }

    // === Helpers ===

    fn rendering_enabled(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn bg_and_sprites_enabled(&self) -> bool {
        self.mask & 0x08 != 0 && self.mask & 0x10 != 0
    }

    /// Apply PPUMASK greyscale (bit 0) and emphasis (bits 5-7) to an ARGB colour.
    ///
    /// Greyscale forces the palette index to column 0 (AND with $30) before
    /// lookup. Emphasis attenuates the *other* channels: emphasise-red dims
    /// green and blue, etc. The attenuation factor is ~0.816 per NES Dev wiki.
    fn apply_mask_effects(&self, palette_index: u8) -> u32 {
        let idx = if self.mask & 0x01 != 0 {
            (palette_index & 0x30) as usize
        } else {
            palette_index as usize
        };

        let argb = PALETTE[idx];
        let emphasis = self.mask >> 5;
        if emphasis == 0 {
            return argb;
        }

        // NTSC emphasis bits: bit 0 = red, bit 1 = green, bit 2 = blue.
        // Each set bit attenuates the OTHER two channels.
        let mut r = (argb >> 16) & 0xFF;
        let mut g = (argb >> 8) & 0xFF;
        let mut b = argb & 0xFF;

        // Emphasise red → dim green and blue
        if emphasis & 0x01 != 0 {
            g = g * 13 / 16;
            b = b * 13 / 16;
        }
        // Emphasise green → dim red and blue
        if emphasis & 0x02 != 0 {
            r = r * 13 / 16;
            b = b * 13 / 16;
        }
        // Emphasise blue → dim red and green
        if emphasis & 0x04 != 0 {
            r = r * 13 / 16;
            g = g * 13 / 16;
        }

        0xFF00_0000 | (r << 16) | (g << 8) | b
    }

    fn check_nmi(&mut self) {
        let nmi_active = self.nmi_occurred && self.nmi_output;
        if nmi_active && !self.nmi_edge {
            self.nmi_edge = true;
        } else if !nmi_active {
            self.nmi_edge = false;
        }
    }

    /// Take the pending NMI flag (used by the NES tick loop to signal CPU).
    pub fn take_nmi(&mut self) -> bool {
        if self.nmi_edge {
            self.nmi_edge = false;
            true
        } else {
            false
        }
    }

    /// Write OAM data (for DMA).
    pub fn write_oam(&mut self, offset: u8, value: u8) {
        self.oam[offset as usize] = value;
    }

    /// Read OAM data (for observation).
    #[must_use]
    pub fn read_oam(&self, offset: u8) -> u8 {
        self.oam[offset as usize]
    }

    /// Reference to the framebuffer (ARGB32, 256x240).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Current scanline.
    #[must_use]
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Current dot.
    #[must_use]
    pub fn dot(&self) -> u16 {
        self.dot
    }

    /// OAM address register.
    #[must_use]
    pub fn oam_addr(&self) -> u8 {
        self.oam_addr
    }

    /// Read nametable RAM directly (for observation/screen text).
    #[must_use]
    pub fn read_nametable(&self, addr: u16) -> u8 {
        self.nametable_ram[(addr as usize) & 0x7FF]
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

/// Reverse the bits in a byte (for horizontal sprite flip).
fn flip_byte(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    (b & 0xAA) >> 1 | (b & 0x55) << 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_byte_works() {
        assert_eq!(flip_byte(0b1000_0000), 0b0000_0001);
        assert_eq!(flip_byte(0b1010_0101), 0b1010_0101);
        assert_eq!(flip_byte(0xFF), 0xFF);
        assert_eq!(flip_byte(0x00), 0x00);
    }

    #[test]
    fn palette_mirroring() {
        let ppu = Ppu::new();
        assert_eq!(ppu.mirror_palette_addr(0x3F10), 0x00);
        assert_eq!(ppu.mirror_palette_addr(0x3F14), 0x04);
        assert_eq!(ppu.mirror_palette_addr(0x3F01), 0x01);
        assert_eq!(ppu.mirror_palette_addr(0x3F1F), 0x1F);
        // $3F1C mirrors to $3F0C
        assert_eq!(ppu.mirror_palette_addr(0x3F1C), 0x0C);
    }

    #[test]
    fn nametable_mirroring_horizontal() {
        let ppu = Ppu::new();
        // NT 0 and NT 1 → page 0
        let a0 = ppu.mirror_nametable_addr(0x2000, Mirroring::Horizontal);
        let a1 = ppu.mirror_nametable_addr(0x2400, Mirroring::Horizontal);
        assert_eq!(a0, 0);
        assert_eq!(a1, 0);
        // NT 2 and NT 3 → page 1
        let a2 = ppu.mirror_nametable_addr(0x2800, Mirroring::Horizontal);
        let a3 = ppu.mirror_nametable_addr(0x2C00, Mirroring::Horizontal);
        assert_eq!(a2, 0x0400);
        assert_eq!(a3, 0x0400);
    }

    fn dummy_mapper() -> crate::cartridge::Nrom {
        crate::cartridge::Nrom::new(vec![0u8; 32768], vec![0u8; 8192], Mirroring::Horizontal)
    }

    #[test]
    fn sprite_overflow_bug_skips_real_overflow() {
        // The 2C02 bug: after 8 sprites, the byte offset `m` increments
        // alongside `n` on each miss. The buggy loop starts at m=0 for the
        // first sprite checked. If that sprite's Y is out of range, m
        // becomes 1 — now the PPU reads tile bytes as Y. Real overflows
        // get missed when the non-Y bytes are out of range.
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 50;
        ppu.ctrl = 0; // 8x8 sprites

        // First 8 sprites: Y=50 (in range)
        for i in 0..8 {
            ppu.oam[i * 4] = 50;
        }
        // 9th sprite (index 8): Y=50 (triggers overflow evaluation)
        ppu.oam[8 * 4] = 50;

        // The buggy loop starts at n=9, m=0. After each miss, both n and m
        // increment. m wraps every 4 entries, so the hardware reads the
        // actual Y byte at n=9,13,17,... (m=0) and non-Y bytes elsewhere.
        //
        // Place out-of-range Y at m=0 positions (9,13,17,...) so those
        // miss. Place in-range Y=50 at all other positions — these sprites
        // are genuinely on the scanline but the bug reads their non-Y
        // bytes (tile/attr/X = 200) instead of Y.
        for i in 9..64 {
            let m_at_i = (i - 9) & 3;
            if m_at_i == 0 {
                // m=0: hardware reads actual Y byte
                ppu.oam[i * 4] = 200; // out of range → miss
            } else {
                // m!=0: hardware reads non-Y byte as "Y"
                ppu.oam[i * 4] = 50; // genuinely in range (but never read)
            }
            ppu.oam[i * 4 + 1] = 200; // tile
            ppu.oam[i * 4 + 2] = 200; // attr
            ppu.oam[i * 4 + 3] = 200; // X
        }

        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 8);
        // Overflow flag NOT set: real sprites at Y=50 are missed because
        // the buggy m offset reads 200 instead of 50.
        assert_eq!(ppu.status & 0x20, 0, "overflow flag set despite bug");
    }

    #[test]
    fn sprite_overflow_bug_false_positive() {
        // When the buggy m offset happens to read a byte that looks like
        // a Y in range, the PPU sets the overflow flag — a false positive.
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 50;
        ppu.ctrl = 0; // 8x8 sprites

        // First 8 sprites on scanline 50
        for i in 0..8 {
            ppu.oam[i * 4] = 50;
        }
        // 9th sprite triggers overflow evaluation
        ppu.oam[8 * 4] = 50;

        // Sprite 9: Y=200 (m=0, not in range → m becomes 1)
        ppu.oam[9 * 4] = 200;
        ppu.oam[9 * 4 + 1] = 200;
        ppu.oam[9 * 4 + 2] = 200;
        ppu.oam[9 * 4 + 3] = 200;

        // Sprite 10: Y=200 (not in range at m=0, but m=1 here).
        // Hardware reads tile byte as "Y". Set tile byte to 50 → match!
        ppu.oam[10 * 4] = 200; // actual Y (never read — m=1)
        ppu.oam[10 * 4 + 1] = 50; // tile byte read as "Y" → false positive
        ppu.oam[10 * 4 + 2] = 200;
        ppu.oam[10 * 4 + 3] = 200;

        // Fill rest far away
        for i in 11..64 {
            ppu.oam[i * 4] = 200;
            ppu.oam[i * 4 + 1] = 200;
            ppu.oam[i * 4 + 2] = 200;
            ppu.oam[i * 4 + 3] = 200;
        }

        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 8);
        // Overflow flag IS set: tile byte 50 falsely matches scanline 50
        assert_ne!(
            ppu.status & 0x20,
            0,
            "overflow flag not set on false positive"
        );
    }

    #[test]
    fn nametable_mirroring_vertical() {
        let ppu = Ppu::new();
        // NT 0 and NT 2 → page 0
        let a0 = ppu.mirror_nametable_addr(0x2000, Mirroring::Vertical);
        let a2 = ppu.mirror_nametable_addr(0x2800, Mirroring::Vertical);
        assert_eq!(a0, 0);
        assert_eq!(a2, 0);
        // NT 1 and NT 3 → page 1
        let a1 = ppu.mirror_nametable_addr(0x2400, Mirroring::Vertical);
        let a3 = ppu.mirror_nametable_addr(0x2C00, Mirroring::Vertical);
        assert_eq!(a1, 0x0400);
        assert_eq!(a3, 0x0400);
    }

    #[test]
    fn greyscale_masks_palette_column() {
        let mut ppu = Ppu::new();
        // Greyscale off: palette index 0x15 maps to PALETTE[0x15]
        ppu.mask = 0x00;
        let normal = ppu.apply_mask_effects(0x15);
        assert_eq!(normal, PALETTE[0x15]);

        // Greyscale on: palette index 0x15 → 0x15 & 0x30 = 0x10
        ppu.mask = 0x01;
        let grey = ppu.apply_mask_effects(0x15);
        assert_eq!(grey, PALETTE[0x10]);
    }

    #[test]
    fn emphasis_red_dims_green_and_blue() {
        let mut ppu = Ppu::new();
        // Emphasis red = PPUMASK bit 5
        ppu.mask = 0x20;
        let argb = ppu.apply_mask_effects(0x20); // A known palette entry

        let base = PALETTE[0x20];
        let base_r = (base >> 16) & 0xFF;
        let base_g = (base >> 8) & 0xFF;
        let base_b = base & 0xFF;

        let out_r = (argb >> 16) & 0xFF;
        let out_g = (argb >> 8) & 0xFF;
        let out_b = argb & 0xFF;

        // Red unchanged, green and blue attenuated
        assert_eq!(out_r, base_r);
        assert_eq!(out_g, base_g * 13 / 16);
        assert_eq!(out_b, base_b * 13 / 16);
    }

    #[test]
    fn no_emphasis_returns_raw_palette() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x00;
        for idx in 0..64u8 {
            assert_eq!(ppu.apply_mask_effects(idx), PALETTE[idx as usize]);
        }
    }
}
