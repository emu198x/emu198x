//! Ricoh 2C02 PPU (Picture Processing Unit).
//!
//! Dot-based rendering. One [`Ppu::tick()`] = one PPU dot. The PPU
//! runs at 5,369,318 Hz (21,477,272 / 4). Each frame is 341 dots ×
//! 262 scanlines (NTSC).
//!
//! ## Scanline layout
//! - 0-239: visible scanlines (render pixels)
//! - 240: post-render (idle)
//! - 241-260: VBlank
//! - 261: pre-render
//!
//! ## Interface changes from the archive
//!
//! The archive PPU took `chr_read: &mut dyn FnMut(u16) -> u8` and
//! `mirroring: Mirroring` closures on every method. This port takes
//! `&mut dyn Mapper` instead, matching the pin contract in
//! [nes-clock-topology.md](../../wiki/decisions/nes-clock-topology.md).
//! The mapper provides CHR reads/writes and mirroring — the PPU
//! calls through it directly.
//!
//! NMI is a public `bool` field rather than an internal pin with
//! edge-detection helpers. The machine layer routes `ppu.nmi` →
//! `cpu.nmi` between ticks; the CPU's own edge detector handles
//! the rest. A12 transitions during rendering call
//! `mapper.notify_a12_rendering()` directly from inside `tick()`
//! instead of being deferred for the machine to poll.

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

pub mod palette;

use format_nintendo_nes_ines::Mapper;
pub use format_nintendo_nes_ines::Mirroring;
use palette::PALETTE;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Framebuffer dimensions.
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 240;

/// PPU 2C02.
#[derive(Clone, Serialize, Deserialize)]
pub struct Ppu {
    // ── VRAM ────────────────────────────────────────────────────
    #[serde(with = "BigArray")]
    nametable_ram: [u8; 2048],
    palette_ram: [u8; 32],
    #[serde(with = "BigArray")]
    oam: [u8; 256],

    // ── Registers ───────────────────────────────────────────────
    ctrl: u8,
    mask: u8,
    status: u8,
    oam_addr: u8,

    // ── Loopy scroll/address registers ──────────────────────────
    v: u16,
    t: u16,
    fine_x: u8,
    w: bool,

    // ── Data read buffer ($2007) ────────────────────────────────
    read_buffer: u8,
    /// Open bus latch: last value written to any PPU register.
    open_bus: u8,

    // ── Rendering position ──────────────────────────────────────
    scanline: u16,
    dot: u16,
    frame_odd: bool,

    // ── Background shift registers ──────────────────────────────
    bg_shift_pattern_lo: u16,
    bg_shift_pattern_hi: u16,
    bg_shift_attrib_lo: u16,
    bg_shift_attrib_hi: u16,
    bg_next_tile_id: u8,
    bg_next_tile_attrib: u8,
    bg_next_tile_lo: u8,
    bg_next_tile_hi: u8,

    // ── Sprite evaluation ───────────────────────────────────────
    secondary_oam: [u8; 32],
    sprite_count: u8,
    sprite_patterns_lo: [u8; 8],
    sprite_patterns_hi: [u8; 8],
    sprite_attribs: [u8; 8],
    sprite_x_counters: [u8; 8],
    sprite_zero_on_line: bool,

    // ── Output ──────────────────────────────────────────────────
    framebuffer: Vec<u32>,

    // ── NMI state ───────────────────────────────────────────────
    nmi_occurred: bool,
    nmi_output: bool,

    /// **NMI output pin.** `true` when the PPU is asserting NMI
    /// (VBlank active AND NMI enabled via $2000 bit 7). The
    /// machine layer routes this to `cpu.nmi` between ticks.
    ///
    /// Unlike the archive (which used active-low `/NMI` semantics
    /// with edge detection helpers), this is active-high to match
    /// the convention used by the mos-6502 crate's `nmi` input
    /// field — `true` means "NMI is being requested".
    pub nmi: bool,

    // ── Configuration ───────────────────────────────────────────
    /// Pre-render scanline number (261 for NTSC, 311 for PAL).
    pre_render_line: u16,
    /// Suppress VBL flag on the next tick. Set when $2002 is read
    /// on the exact PPU cycle that VBL would be set.
    suppress_vbl: bool,
    /// Current address on the PPU address bus. Updated every dot
    /// during rendering. Bit 12 (A12) is fed to the mapper for
    /// MMC3-style scanline counters.
    bus_address: u16,
    /// Previous A12 state, used for edge detection on the bus
    /// address line during rendering.
    prev_a12: bool,
    /// Pending `nmi_output` value from a `$2000` write. Committed
    /// by [`flush_nmi_line()`](Ppu::flush_nmi_line) after all PPU
    /// dots in the current CPU cycle have run — preserving the
    /// 1-cycle delay for `$2000` writes.
    pending_nmi_output: Option<bool>,
}

impl Ppu {
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_pre_render_line(261)
    }

    /// Create a PPU with the given pre-render scanline number.
    /// NTSC: 261, PAL: 311.
    #[must_use]
    pub fn new_with_pre_render_line(pre_render_line: u16) -> Self {
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
            open_bus: 0,

            scanline: pre_render_line,
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
            nmi: false,

            pre_render_line,
            suppress_vbl: false,
            bus_address: 0,
            prev_a12: false,
            pending_nmi_output: None,
        }
    }

    // ════════════════════════════════════════════════════════════
    //  Tick — one PPU dot
    // ════════════════════════════════════════════════════════════

    /// Advance the PPU by one dot.
    ///
    /// The machine layer calls this once per PPU clock (every
    /// master clock division). The mapper is passed by reference
    /// so the PPU can read CHR ROM/RAM and query mirroring.
    pub fn tick(&mut self, mapper: &mut dyn Mapper) {
        // Pre-render line
        if self.scanline == self.pre_render_line {
            self.tick_prerender(mapper);
        }
        // Visible scanlines (0-239)
        else if self.scanline <= 239 {
            self.tick_visible(mapper);
        }
        // Post-render / VBlank: bus holds current VRAM address
        else {
            self.bus_address = self.v & 0x3FFF;
        }

        // VBlank start: VBL flag at dot 1, NMI pin at dot 3
        // (2-dot pipeline delay on real hardware).
        if self.scanline == 241 && self.dot == 1 {
            if self.suppress_vbl {
                self.suppress_vbl = false;
            } else {
                self.status |= 0x80;
            }
        }
        if self.scanline == 241 && self.dot == 3 && self.status & 0x80 != 0 {
            self.nmi_occurred = true;
            self.update_nmi_line();
        }

        // Notify mapper of A12 transitions during rendering.
        self.check_a12(mapper);

        // Advance dot/scanline.
        self.dot += 1;
        if self.dot > 340 {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline > self.pre_render_line {
                self.scanline = 0;
                self.frame_odd = !self.frame_odd;
            }
        }
    }

    fn tick_prerender(&mut self, mapper: &mut dyn Mapper) {
        if self.dot == 1 {
            // Clear VBlank flag, sprite 0 hit, sprite overflow.
            self.status &= 0x1F;
            self.sprite_patterns_lo = [0; 8];
            self.sprite_patterns_hi = [0; 8];
        }
        // Clear nmi_occurred at dot 3 — same 2-dot pipeline delay.
        if self.dot == 3 {
            self.nmi_occurred = false;
            self.update_nmi_line();
        }

        if self.rendering_enabled() {
            if self.dot >= 257 && self.dot <= 320 {
                self.update_sprite_bus_address();
            }
            if (self.dot >= 1 && self.dot <= 256) || (self.dot >= 321 && self.dot <= 340) {
                self.bg_fetch_cycle(mapper);
                self.shift_registers();
            }

            if self.dot == 256 {
                self.increment_y();
            }
            if self.dot == 257 {
                self.copy_horizontal();
            }
            // Copy vertical bits from t to v during dots 280-304.
            if self.dot >= 280 && self.dot <= 304 {
                self.copy_vertical();
            }
            // Odd frame skip: skip last dot on odd frames.
            if self.dot == 339 && self.frame_odd {
                self.dot = 340;
            }
        }
    }

    fn tick_visible(&mut self, mapper: &mut dyn Mapper) {
        if self.rendering_enabled() {
            if self.dot >= 1 && self.dot <= 256 {
                self.render_pixel();
                self.bg_fetch_cycle(mapper);
                self.shift_registers();
            }
            if self.dot == 257 {
                self.evaluate_sprites(mapper);
            }
            if self.dot >= 257 && self.dot <= 320 {
                self.update_sprite_bus_address();
            }
            if self.dot >= 321 && self.dot <= 340 {
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
            // Rendering disabled: output background colour.
            let bg_colour = self.palette_ram[0] & 0x3F;
            let x = (self.dot - 1) as usize;
            let y = self.scanline as usize;
            if y < FB_HEIGHT as usize && x < FB_WIDTH as usize {
                self.framebuffer[y * FB_WIDTH as usize + x] = self.apply_mask_effects(bg_colour);
            }
        }
    }

    // ════════════════════════════════════════════════════════════
    //  Background fetch pipeline
    // ════════════════════════════════════════════════════════════

    fn bg_fetch_cycle(&mut self, mapper: &mut dyn Mapper) {
        let cycle = if self.dot >= 321 {
            self.dot - 321
        } else {
            self.dot - 1
        };

        match cycle & 0x07 {
            0 => {
                if self.dot != 321 {
                    self.load_bg_shift_registers();
                }
                let nt_addr = 0x2000 | (self.v & 0x0FFF);
                self.bg_next_tile_id = self.ppu_read(nt_addr, mapper);
            }
            2 => {
                let attr_addr =
                    0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07);
                let attr_byte = self.ppu_read(attr_addr, mapper);
                let shift = ((self.v >> 4) & 0x04) | (self.v & 0x02);
                self.bg_next_tile_attrib = (attr_byte >> shift) & 0x03;
            }
            4 => {
                let bg_table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0 };
                let fine_y = (self.v >> 12) & 0x07;
                let addr = bg_table + u16::from(self.bg_next_tile_id) * 16 + fine_y;
                self.bg_next_tile_lo = self.ppu_read(addr, mapper);
            }
            6 => {
                let bg_table = if self.ctrl & 0x10 != 0 { 0x1000u16 } else { 0 };
                let fine_y = (self.v >> 12) & 0x07;
                let addr = bg_table + u16::from(self.bg_next_tile_id) * 16 + fine_y + 8;
                self.bg_next_tile_hi = self.ppu_read(addr, mapper);
            }
            7 => {
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

    // ════════════════════════════════════════════════════════════
    //  Pixel output
    // ════════════════════════════════════════════════════════════

    fn render_pixel(&mut self) {
        let x = (self.dot - 1) as usize;
        let y = self.scanline as usize;
        if y >= FB_HEIGHT as usize || x >= FB_WIDTH as usize {
            return;
        }

        let (bg_pixel, bg_palette) = self.get_bg_pixel();
        let (sp_pixel, sp_palette, sp_priority, sp_is_zero) = self.get_sprite_pixel(x);

        let (pixel, palette) = match (bg_pixel, sp_pixel) {
            (0, 0) => (0, 0),
            (0, _) => (sp_pixel, sp_palette),
            (_, 0) => (bg_pixel, bg_palette),
            (_, _) => {
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

            let palette = (self.sprite_attribs[i] & 0x03) + 4;
            let behind_bg = self.sprite_attribs[i] & 0x20 != 0;
            let is_sprite_zero = self.sprite_zero_on_line && i == 0;

            return (pixel, palette, behind_bg, is_sprite_zero);
        }

        (0, 0, false, false)
    }

    // ════════════════════════════════════════════════════════════
    //  Sprite evaluation
    // ════════════════════════════════════════════════════════════

    fn evaluate_sprites(&mut self, mapper: &mut dyn Mapper) {
        let sprite_height: u16 = if self.ctrl & 0x20 != 0 { 16 } else { 8 };
        let next_scanline = self.scanline;

        self.secondary_oam = [0xFF; 32];
        self.sprite_count = 0;
        self.sprite_zero_on_line = false;

        for i in 0..64u8 {
            let y = u16::from(self.oam[i as usize * 4]);
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
                    // 2C02 hardware bug: after finding 8 sprites, the
                    // PPU continues scanning but increments the OAM
                    // byte offset (m) alongside the sprite index (n) on
                    // each miss. This causes it to compare tile,
                    // attribute, or X bytes as if they were Y
                    // coordinates — missing real overflows and producing
                    // false positives.
                    let mut n = (i + 1) as usize;
                    let mut m: usize = 0;
                    while n < 64 {
                        let byte = u16::from(self.oam[(n * 4 + m) & 0xFF]);
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

        // Fetch sprite patterns.
        for i in 0..8usize {
            if i < self.sprite_count as usize {
                let sprite_y = u16::from(self.secondary_oam[i * 4]);
                let tile_index = self.secondary_oam[i * 4 + 1];
                let attribs = self.secondary_oam[i * 4 + 2];
                let sprite_x = self.secondary_oam[i * 4 + 3];

                let flip_v = attribs & 0x80 != 0;
                let mut row = next_scanline.wrapping_sub(sprite_y);

                let (table, tile, sprite_row) = if sprite_height == 16 {
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
                    let table = if self.ctrl & 0x08 != 0 { 0x1000u16 } else { 0 };
                    if flip_v {
                        row = 7 - row;
                    }
                    (table, tile_index, row)
                };

                let addr = table + u16::from(tile) * 16 + sprite_row;
                self.bus_address = addr;
                let mut lo = mapper.chr_read(addr);
                let mut hi = mapper.chr_read(addr + 8);

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

    /// Compute the PPU bus address for sprite tile fetch dots
    /// (257-320). Each sprite takes 8 dots: 2 garbage NT, 2
    /// garbage attr, 2 pattern lo, 2 pattern hi.
    fn update_sprite_bus_address(&mut self) {
        let sprite_idx = ((self.dot - 257) / 8) as usize;
        let phase = (self.dot - 257) % 8;

        match phase {
            0 | 1 => {
                self.bus_address = 0x2000 | (self.v & 0x0FFF);
            }
            2 | 3 => {
                self.bus_address =
                    0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07);
            }
            4..=7 => {
                let high_byte = phase >= 6;
                self.bus_address = self.sprite_fetch_addr(sprite_idx, high_byte);
            }
            _ => unreachable!(),
        }
    }

    fn sprite_fetch_addr(&self, sprite_idx: usize, high_byte: bool) -> u16 {
        let sprite_height: u16 = if self.ctrl & 0x20 != 0 { 16 } else { 8 };

        let (tile_index, sprite_y) = if sprite_idx < self.sprite_count as usize {
            (
                self.secondary_oam[sprite_idx * 4 + 1],
                u16::from(self.secondary_oam[sprite_idx * 4]),
            )
        } else {
            (0xFF, 0xFF)
        };

        let row = if sprite_idx < self.sprite_count as usize {
            let attribs = self.secondary_oam[sprite_idx * 4 + 2];
            let flip_v = attribs & 0x80 != 0;
            let mut r = self.scanline.wrapping_sub(sprite_y);
            if sprite_height == 16 {
                if flip_v {
                    r = 15u16.wrapping_sub(r);
                }
            } else if flip_v {
                r = 7u16.wrapping_sub(r);
            }
            r
        } else {
            0
        };

        let (table, tile, sprite_row) = if sprite_height == 16 {
            let table = u16::from(tile_index & 1) * 0x1000;
            let tile = tile_index & 0xFE;
            if row >= 8 {
                (table, tile + 1, row - 8)
            } else {
                (table, tile, row)
            }
        } else {
            let table = if self.ctrl & 0x08 != 0 { 0x1000u16 } else { 0 };
            (table, tile_index, row)
        };

        let mut addr = table
            .wrapping_add(u16::from(tile).wrapping_mul(16))
            .wrapping_add(sprite_row);
        if high_byte {
            addr = addr.wrapping_add(8);
        }
        addr
    }

    // ════════════════════════════════════════════════════════════
    //  Scrolling
    // ════════════════════════════════════════════════════════════

    fn increment_x(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            self.v += 1;
        }
    }

    fn increment_y(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        if (self.v & 0x7000) == 0x7000 {
            self.v &= !0x7000;
            let mut coarse_y = (self.v & 0x03E0) >> 5;
            if coarse_y == 29 {
                coarse_y = 0;
                self.v ^= 0x0800;
            } else if coarse_y == 31 {
                coarse_y = 0;
            } else {
                coarse_y += 1;
            }
            self.v = (self.v & !0x03E0) | (coarse_y << 5);
        } else {
            self.v += 0x1000;
        }
    }

    fn copy_horizontal(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }

    fn copy_vertical(&mut self) {
        if !self.rendering_enabled() {
            return;
        }
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    // ════════════════════════════════════════════════════════════
    //  Register access (CPU side)
    // ════════════════════════════════════════════════════════════

    /// CPU read from PPU register (`$2000-$2007` mirrored).
    pub fn cpu_read(&mut self, reg: u16, mapper: &mut dyn Mapper) -> u8 {
        let result = match reg & 0x07 {
            // $2002 - PPUSTATUS
            2 => {
                if self.scanline == 241 && self.dot == 1 {
                    self.suppress_vbl = true;
                }
                let result = (self.status & 0xE0) | (self.open_bus & 0x1F);
                self.status &= !0x80;
                self.nmi_occurred = false;
                self.update_nmi_line();
                self.w = false;
                result
            }
            // $2004 - OAMDATA
            4 => self.oam[self.oam_addr as usize],
            // $2007 - PPUDATA
            7 => {
                let addr = self.v & 0x3FFF;
                let mut result = self.read_buffer;
                self.read_buffer = self.ppu_read(addr, mapper);
                if addr >= 0x3F00 {
                    result = self.palette_ram[self.mirror_palette_addr(addr) as usize];
                    self.read_buffer = self.ppu_read(addr & 0x2FFF, mapper);
                }
                let new_v = self
                    .v
                    .wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 })
                    & 0x7FFF;
                self.set_v(new_v);
                result
            }
            _ => self.open_bus,
        };
        self.open_bus = result;
        result
    }

    /// CPU write to PPU register (`$2000-$2007` mirrored).
    pub fn cpu_write(&mut self, reg: u16, val: u8, mapper: &mut dyn Mapper) {
        self.open_bus = val;
        match reg & 0x07 {
            // $2000 - PPUCTRL
            0 => {
                self.ctrl = val;
                self.t = (self.t & !0x0C00) | (u16::from(val & 0x03) << 10);
                self.pending_nmi_output = Some(val & 0x80 != 0);
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
                if self.w {
                    self.t = (self.t & !0x73E0)
                        | (u16::from(val & 0x07) << 12)
                        | (u16::from(val >> 3) << 5);
                } else {
                    self.t = (self.t & !0x001F) | (u16::from(val) >> 3);
                    self.fine_x = val & 0x07;
                }
                self.w = !self.w;
            }
            // $2006 - PPUADDR
            6 => {
                if self.w {
                    self.t = (self.t & 0xFF00) | u16::from(val);
                    self.set_v(self.t);
                } else {
                    self.t = (self.t & 0x00FF) | (u16::from(val & 0x3F) << 8);
                }
                self.w = !self.w;
            }
            // $2007 - PPUDATA
            7 => {
                let addr = self.v & 0x3FFF;
                self.ppu_write(addr, val, mapper);
                let new_v = self
                    .v
                    .wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 })
                    & 0x7FFF;
                self.set_v(new_v);
            }
            _ => {}
        }
    }

    // ════════════════════════════════════════════════════════════
    //  PPU memory access
    // ════════════════════════════════════════════════════════════

    fn ppu_read(&mut self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        let addr = addr & 0x3FFF;
        self.bus_address = addr;
        match addr {
            0x0000..=0x1FFF => mapper.chr_read(addr),
            0x2000..=0x3EFF => {
                if let Some(value) = mapper.nametable_read(addr) {
                    return value;
                }
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
                if mapper.nametable_write(addr, val) {
                    return;
                }
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
        let nt_addr = (addr - 0x2000) & 0x0FFF;
        match mirroring {
            Mirroring::Horizontal => {
                let page = (nt_addr / 0x0800) * 0x0400;
                page + (nt_addr & 0x03FF)
            }
            Mirroring::Vertical => nt_addr & 0x07FF,
            Mirroring::FourScreen => nt_addr & 0x0FFF,
            Mirroring::SingleScreenLower => nt_addr & 0x03FF,
            Mirroring::SingleScreenUpper => 0x0400 + (nt_addr & 0x03FF),
        }
    }

    fn mirror_palette_addr(&self, addr: u16) -> u16 {
        let mut a = (addr - 0x3F00) & 0x1F;
        if a == 0x10 || a == 0x14 || a == 0x18 || a == 0x1C {
            a -= 0x10;
        }
        a
    }

    // ════════════════════════════════════════════════════════════
    //  Helpers
    // ════════════════════════════════════════════════════════════

    fn rendering_enabled(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn bg_and_sprites_enabled(&self) -> bool {
        self.mask & 0x08 != 0 && self.mask & 0x10 != 0
    }

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

        let mut r = (argb >> 16) & 0xFF;
        let mut g = (argb >> 8) & 0xFF;
        let mut b = argb & 0xFF;

        if emphasis & 0x01 != 0 {
            g = g * 13 / 16;
            b = b * 13 / 16;
        }
        if emphasis & 0x02 != 0 {
            r = r * 13 / 16;
            b = b * 13 / 16;
        }
        if emphasis & 0x04 != 0 {
            r = r * 13 / 16;
            g = g * 13 / 16;
        }

        0xFF00_0000 | (r << 16) | (g << 8) | b
    }

    // ════════════════════════════════════════════════════════════
    //  NMI line management
    // ════════════════════════════════════════════════════════════

    /// Update the public `nmi` field from internal state.
    /// Active-high: `true` = NMI requested.
    fn update_nmi_line(&mut self) {
        self.nmi = self.nmi_occurred && self.nmi_output;
    }

    /// Commit any deferred `nmi_output` change from a `$2000`
    /// write. Call after all PPU dots in the current CPU cycle
    /// have run and before the machine routes `ppu.nmi` →
    /// `cpu.nmi`. This preserves the 1-cycle delay for `$2000`
    /// writes.
    pub fn flush_nmi_line(&mut self) {
        if let Some(nmi_output) = self.pending_nmi_output.take() {
            self.nmi_output = nmi_output;
            self.update_nmi_line();
        }
    }

    // ════════════════════════════════════════════════════════════
    //  A12 change notification
    // ════════════════════════════════════════════════════════════

    /// Check whether A12 changed on the PPU address bus and notify
    /// the mapper if it did. Called once per dot from `tick()`.
    fn check_a12(&mut self, mapper: &mut dyn Mapper) {
        let a12 = self.bus_address & 0x1000 != 0;
        if a12 != self.prev_a12 {
            self.prev_a12 = a12;
            if self.rendering_active() {
                mapper.notify_a12_rendering(a12);
            }
        }
    }

    /// Whether the PPU is actively rendering (visible or
    /// pre-render line with rendering enabled).
    fn rendering_active(&self) -> bool {
        self.rendering_enabled() && (self.scanline <= 239 || self.scanline == self.pre_render_line)
    }

    /// Update `v` register and track A12 transitions.
    fn set_v(&mut self, new_v: u16) {
        self.v = new_v;
        // A12 edge detection happens centrally in check_a12(),
        // which runs once per dot. For register writes ($2006,
        // $2007) that change v mid-dot, the edge will be picked
        // up on the next check_a12() call.
    }

    // ════════════════════════════════════════════════════════════
    //  OAM DMA support
    // ════════════════════════════════════════════════════════════

    /// Write OAM data (for OAMDMA).
    pub fn write_oam(&mut self, offset: u8, value: u8) {
        self.oam[offset as usize] = value;
    }

    /// Read OAM data (for observation).
    #[must_use]
    pub fn read_oam(&self, offset: u8) -> u8 {
        self.oam[offset as usize]
    }

    // ════════════════════════════════════════════════════════════
    //  Observation accessors
    // ════════════════════════════════════════════════════════════

    /// Reference to the framebuffer (ARGB32, 256×240).
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

    /// Whether the current frame is odd.
    #[must_use]
    pub fn frame_odd(&self) -> bool {
        self.frame_odd
    }

    /// Pre-render scanline number (261 NTSC, 311 PAL).
    #[must_use]
    pub fn pre_render_line(&self) -> u16 {
        self.pre_render_line
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

    /// Palette RAM (32 bytes).
    #[must_use]
    pub fn palette_ram(&self) -> &[u8; 32] {
        &self.palette_ram
    }

    /// OAM (256 bytes).
    #[must_use]
    pub fn oam(&self) -> &[u8; 256] {
        &self.oam
    }

    /// PPUCTRL register ($2000).
    #[must_use]
    pub fn ctrl(&self) -> u8 {
        self.ctrl
    }

    /// PPUMASK register ($2001).
    #[must_use]
    pub fn mask(&self) -> u8 {
        self.mask
    }

    /// PPUSTATUS register ($2002) — raw internal value.
    #[must_use]
    pub fn status(&self) -> u8 {
        self.status
    }

    /// Loopy V register.
    #[must_use]
    pub fn v_reg(&self) -> u16 {
        self.v
    }

    /// Loopy T register.
    #[must_use]
    pub fn t_reg(&self) -> u16 {
        self.t
    }

    /// Fine X scroll (3 bits).
    #[must_use]
    pub fn fine_x(&self) -> u8 {
        self.fine_x
    }

    /// Write toggle (W latch).
    #[must_use]
    pub fn w_latch(&self) -> bool {
        self.w
    }

    /// Open bus latch value.
    #[must_use]
    pub fn open_bus(&self) -> u8 {
        self.open_bus
    }

    /// Nametable RAM (2 KiB).
    #[must_use]
    pub fn nametable_ram(&self) -> &[u8; 2048] {
        &self.nametable_ram
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

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use format_nintendo_nes_ines::Nrom;

    /// Build a minimal NROM mapper with 8 KiB of zero CHR RAM and
    /// horizontal mirroring. Good enough for any test that doesn't
    /// care about CHR tile content.
    fn dummy_mapper() -> Nrom {
        Nrom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal)
    }

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
        assert_eq!(ppu.mirror_palette_addr(0x3F1C), 0x0C);
    }

    #[test]
    fn nametable_mirroring_horizontal() {
        let ppu = Ppu::new();
        let a0 = ppu.mirror_nametable_addr(0x2000, Mirroring::Horizontal);
        let a1 = ppu.mirror_nametable_addr(0x2400, Mirroring::Horizontal);
        assert_eq!(a0, 0);
        assert_eq!(a1, 0);
        let a2 = ppu.mirror_nametable_addr(0x2800, Mirroring::Horizontal);
        let a3 = ppu.mirror_nametable_addr(0x2C00, Mirroring::Horizontal);
        assert_eq!(a2, 0x0400);
        assert_eq!(a3, 0x0400);
    }

    #[test]
    fn nametable_mirroring_vertical() {
        let ppu = Ppu::new();
        let a0 = ppu.mirror_nametable_addr(0x2000, Mirroring::Vertical);
        let a2 = ppu.mirror_nametable_addr(0x2800, Mirroring::Vertical);
        assert_eq!(a0, 0);
        assert_eq!(a2, 0);
        let a1 = ppu.mirror_nametable_addr(0x2400, Mirroring::Vertical);
        let a3 = ppu.mirror_nametable_addr(0x2C00, Mirroring::Vertical);
        assert_eq!(a1, 0x0400);
        assert_eq!(a3, 0x0400);
    }

    #[test]
    fn sprite_overflow_bug_skips_real_overflow() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 50;
        ppu.ctrl = 0;

        for i in 0..8 {
            ppu.oam[i * 4] = 50;
        }
        ppu.oam[8 * 4] = 50;

        for i in 9..64 {
            let m_at_i = (i - 9) & 3;
            if m_at_i == 0 {
                ppu.oam[i * 4] = 200;
            } else {
                ppu.oam[i * 4] = 50;
            }
            ppu.oam[i * 4 + 1] = 200;
            ppu.oam[i * 4 + 2] = 200;
            ppu.oam[i * 4 + 3] = 200;
        }

        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 8);
        assert_eq!(ppu.status & 0x20, 0, "overflow flag set despite bug");
    }

    #[test]
    fn sprite_overflow_bug_false_positive() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 50;
        ppu.ctrl = 0;

        for i in 0..8 {
            ppu.oam[i * 4] = 50;
        }
        ppu.oam[8 * 4] = 50;
        ppu.oam[9 * 4] = 200;
        ppu.oam[9 * 4 + 1] = 200;
        ppu.oam[9 * 4 + 2] = 200;
        ppu.oam[9 * 4 + 3] = 200;
        // Sprite 10: tile byte read as "Y" → false positive.
        ppu.oam[10 * 4] = 200;
        ppu.oam[10 * 4 + 1] = 50;
        ppu.oam[10 * 4 + 2] = 200;
        ppu.oam[10 * 4 + 3] = 200;

        for i in 11..64 {
            ppu.oam[i * 4] = 200;
            ppu.oam[i * 4 + 1] = 200;
            ppu.oam[i * 4 + 2] = 200;
            ppu.oam[i * 4 + 3] = 200;
        }

        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 8);
        assert_ne!(
            ppu.status & 0x20,
            0,
            "overflow flag not set on false positive"
        );
    }

    #[test]
    fn sprite_fetch_address_wraps_invalid_flipped_rows() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x20; // 8x16 sprites
        ppu.scanline = 0;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0xFF; // invalid/out-of-range Y from secondary OAM
        ppu.secondary_oam[1] = 0xFF;
        ppu.secondary_oam[2] = 0x80; // vertical flip

        let addr = ppu.sprite_fetch_addr(0, true);

        assert_eq!(addr, 0x20FE);
    }

    #[test]
    fn greyscale_masks_palette_column() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x00;
        let normal = ppu.apply_mask_effects(0x15);
        assert_eq!(normal, PALETTE[0x15]);

        ppu.mask = 0x01;
        let grey = ppu.apply_mask_effects(0x15);
        assert_eq!(grey, PALETTE[0x10]);
    }

    #[test]
    fn emphasis_red_dims_green_and_blue() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x20;
        let argb = ppu.apply_mask_effects(0x20);

        let base = PALETTE[0x20];
        let base_r = (base >> 16) & 0xFF;
        let base_g = (base >> 8) & 0xFF;
        let base_b = base & 0xFF;

        let out_r = (argb >> 16) & 0xFF;
        let out_g = (argb >> 8) & 0xFF;
        let out_b = argb & 0xFF;

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

    #[test]
    fn vbl_flag_set_at_241_dot_1() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 241;
        ppu.dot = 0;
        ppu.status = 0;

        // Dot 0: no VBL yet.
        ppu.tick(&mut mapper);
        assert_eq!(ppu.status & 0x80, 0);

        // Dot 1: VBL flag should be set.
        ppu.tick(&mut mapper);
        assert_ne!(ppu.status & 0x80, 0, "VBL flag not set at (241, 1)");
    }

    #[test]
    fn nmi_asserted_at_241_dot_3() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 241;
        ppu.dot = 0;
        ppu.status = 0;
        ppu.nmi_output = true; // NMI enabled via $2000

        for _ in 0..3 {
            ppu.tick(&mut mapper);
        }
        assert!(!ppu.nmi, "NMI should not be asserted before dot 3");

        // Dot 3: NMI should fire.
        ppu.tick(&mut mapper);
        assert!(ppu.nmi, "NMI not asserted at (241, 3)");
    }

    #[test]
    fn nmi_not_asserted_when_disabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 241;
        ppu.dot = 0;
        ppu.status = 0;
        ppu.nmi_output = false; // NMI disabled

        for _ in 0..5 {
            ppu.tick(&mut mapper);
        }
        assert!(
            !ppu.nmi,
            "NMI should not assert when NMI output is disabled"
        );
    }

    #[test]
    fn reading_2002_clears_vbl() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.status = 0x80; // VBL flag set

        let val = ppu.cpu_read(0x2002, &mut mapper);
        assert_ne!(val & 0x80, 0, "should read VBL as set");
        assert_eq!(
            ppu.status & 0x80,
            0,
            "VBL flag should be cleared after read"
        );
        assert!(!ppu.nmi, "NMI should be cleared after $2002 read");
    }

    #[test]
    fn ppuscroll_two_write_protocol() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();

        // First write: X scroll = 0x48 → coarse X = 9, fine X = 0
        ppu.cpu_write(0x2005, 0x48, &mut mapper);
        assert_eq!(ppu.fine_x, 0);
        assert_eq!(ppu.t & 0x001F, 9);
        assert!(ppu.w);

        // Second write: Y scroll = 0x20 → coarse Y = 4, fine Y = 0
        ppu.cpu_write(0x2005, 0x20, &mut mapper);
        assert_eq!((ppu.t >> 5) & 0x1F, 4);
        assert_eq!((ppu.t >> 12) & 0x07, 0);
        assert!(!ppu.w);
    }

    #[test]
    fn ppuaddr_two_write_protocol() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();

        ppu.cpu_write(0x2006, 0x21, &mut mapper);
        assert!(ppu.w);
        ppu.cpu_write(0x2006, 0x08, &mut mapper);
        assert!(!ppu.w);
        assert_eq!(ppu.v, 0x2108);
    }

    #[test]
    fn ppudata_write_increments_v() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.v = 0x2000;
        ppu.ctrl = 0; // increment by 1

        ppu.cpu_write(0x2007, 0x42, &mut mapper);
        assert_eq!(ppu.v, 0x2001);

        // Increment by 32 when ctrl bit 2 is set.
        ppu.ctrl = 0x04;
        ppu.cpu_write(0x2007, 0x43, &mut mapper);
        assert_eq!(ppu.v, 0x2021);
    }

    #[test]
    fn prerender_clears_status_flags() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 0;
        ppu.status = 0xE0; // VBL + sprite 0 hit + overflow

        ppu.tick(&mut mapper); // dot 0
        assert_eq!(ppu.status & 0xE0, 0xE0, "flags should persist at dot 0");

        ppu.tick(&mut mapper); // dot 1 — clear
        assert_eq!(
            ppu.status & 0xE0,
            0x00,
            "status flags should be cleared at dot 1 of pre-render"
        );
    }

    #[test]
    fn odd_frame_skip() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18; // rendering enabled
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 338;
        ppu.frame_odd = true;

        ppu.tick(&mut mapper); // dot 338 → 339
        ppu.tick(&mut mapper); // dot 339 — odd frame, should skip to 340
        // After this tick, dot was set to 340 inside tick_prerender,
        // then the post-tick advance makes it 341 which wraps to 0.
        assert_eq!(ppu.dot, 0, "odd frame should skip dot 340 → wrap to 0");
        assert_eq!(
            ppu.scanline, 0,
            "should be on scanline 0 after pre-render wraps"
        );
    }

    #[test]
    fn visible_line_fetches_dots_337_to_340() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = 0;
        ppu.dot = 336;

        for _ in 0..4 {
            ppu.tick(&mut mapper);
        }

        assert_eq!(ppu.dot, 340, "dot should be 340 after ticking from 336");
    }

    #[test]
    fn flush_nmi_line_commits_pending() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.nmi_occurred = true;
        ppu.nmi_output = false;
        ppu.nmi = false;

        // Simulate a $2000 write enabling NMI.
        ppu.cpu_write(0x2000, 0x80, &mut mapper);
        // NMI should not yet be asserted (pending).
        assert!(!ppu.nmi, "NMI should stay deasserted until flush");

        ppu.flush_nmi_line();
        assert!(
            ppu.nmi,
            "NMI should be asserted after flush with nmi_occurred=true"
        );
    }
}
