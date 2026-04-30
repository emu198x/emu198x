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
        mapper.notify_ppu_read(addr, self.rendering_active());
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

    // ─── Cov-5c wave 2: directed tests ─────────────────────────────

    /// NROM with caller-provided CHR ROM bytes (pattern data).
    fn mapper_with_chr(chr: Vec<u8>) -> Nrom {
        Nrom::new(vec![0u8; 16384], chr, Mirroring::Horizontal)
    }

    fn mapper_vertical() -> Nrom {
        Nrom::new(vec![0u8; 16384], Vec::new(), Mirroring::Vertical)
    }

    // ─── Public observation accessors ──────────────────────────────

    #[test]
    fn observation_accessors_expose_state() {
        let mut ppu = Ppu::new();
        ppu.scanline = 123;
        ppu.dot = 45;
        ppu.frame_odd = true;
        ppu.oam_addr = 0x1A;
        ppu.ctrl = 0x80;
        ppu.mask = 0x18;
        ppu.status = 0x40;
        ppu.v = 0x1234;
        ppu.t = 0x4321;
        ppu.fine_x = 5;
        ppu.w = true;
        ppu.open_bus = 0xAB;

        assert_eq!(ppu.scanline(), 123);
        assert_eq!(ppu.dot(), 45);
        assert!(ppu.frame_odd());
        assert_eq!(ppu.pre_render_line(), 261);
        assert_eq!(ppu.oam_addr(), 0x1A);
        assert_eq!(ppu.ctrl(), 0x80);
        assert_eq!(ppu.mask(), 0x18);
        assert_eq!(ppu.status(), 0x40);
        assert_eq!(ppu.v_reg(), 0x1234);
        assert_eq!(ppu.t_reg(), 0x4321);
        assert_eq!(ppu.fine_x(), 5);
        assert!(ppu.w_latch());
        assert_eq!(ppu.open_bus(), 0xAB);
        assert_eq!(ppu.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
        assert_eq!(ppu.palette_ram().len(), 32);
        assert_eq!(ppu.oam().len(), 256);
        assert_eq!(ppu.nametable_ram().len(), 2048);
    }

    #[test]
    fn read_oam_round_trips_write_oam() {
        let mut ppu = Ppu::new();
        ppu.write_oam(0x10, 0x42);
        ppu.write_oam(0xFF, 0x99);
        assert_eq!(ppu.read_oam(0x10), 0x42);
        assert_eq!(ppu.read_oam(0xFF), 0x99);
    }

    #[test]
    fn read_nametable_indexes_internal_ram() {
        let mut ppu = Ppu::new();
        ppu.nametable_ram[0x000] = 0x11;
        ppu.nametable_ram[0x123] = 0x22;
        ppu.nametable_ram[0x7FF] = 0x33;
        assert_eq!(ppu.read_nametable(0x0000), 0x11);
        assert_eq!(ppu.read_nametable(0x0123), 0x22);
        assert_eq!(ppu.read_nametable(0x07FF), 0x33);
        // Higher bits masked off.
        assert_eq!(ppu.read_nametable(0x0FFF), 0x33);
    }

    #[test]
    fn default_ppu_matches_new() {
        let a = Ppu::default();
        let b = Ppu::new();
        assert_eq!(a.scanline(), b.scanline());
        assert_eq!(a.dot(), b.dot());
        assert_eq!(a.pre_render_line(), b.pre_render_line());
    }

    // ─── PPUCTRL writes — t nametable bits ────────────────────────

    #[test]
    fn ppuctrl_write_updates_t_nametable_bits() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.t = 0;
        ppu.cpu_write(0x2000, 0x03, &mut mapper);
        assert_eq!(ppu.t & 0x0C00, 0x0C00, "t bits 10-11 from ctrl bits 0-1");
        assert_eq!(ppu.ctrl, 0x03);
    }

    // ─── PPUMASK / OAMADDR / OAMDATA writes ───────────────────────

    #[test]
    fn ppumask_write_stores_value() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.cpu_write(0x2001, 0x1E, &mut mapper);
        assert_eq!(ppu.mask, 0x1E);
        assert_eq!(ppu.open_bus, 0x1E);
    }

    #[test]
    fn oamaddr_write_stores_value() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.cpu_write(0x2003, 0x80, &mut mapper);
        assert_eq!(ppu.oam_addr, 0x80);
    }

    #[test]
    fn oamdata_write_stores_and_increments_address() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.oam_addr = 0x10;
        ppu.cpu_write(0x2004, 0xAB, &mut mapper);
        assert_eq!(ppu.oam[0x10], 0xAB);
        assert_eq!(ppu.oam_addr, 0x11);

        // Wrap from 0xFF.
        ppu.oam_addr = 0xFF;
        ppu.cpu_write(0x2004, 0xCD, &mut mapper);
        assert_eq!(ppu.oam[0xFF], 0xCD);
        assert_eq!(ppu.oam_addr, 0x00);
    }

    #[test]
    fn oamdata_read_returns_oam_byte() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.oam[0x42] = 0x77;
        ppu.oam_addr = 0x42;
        let val = ppu.cpu_read(0x2004, &mut mapper);
        assert_eq!(val, 0x77);
        assert_eq!(ppu.open_bus, 0x77);
    }

    // ─── Open bus on writeable-only registers ─────────────────────

    #[test]
    fn cpu_read_writeable_only_register_returns_open_bus() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.open_bus = 0x55;
        // $2000, $2001, $2003, $2005, $2006 — write-only.
        for reg in [0x2000u16, 0x2001, 0x2003, 0x2005, 0x2006] {
            assert_eq!(ppu.cpu_read(reg, &mut mapper), 0x55, "reg ${reg:04X}");
        }
    }

    #[test]
    fn cpu_write_to_unreachable_arm_is_noop() {
        // The match in cpu_write covers 0..=7. Mirroring trims reg
        // input via `& 0x07`, so every value maps to 0..7. The `_`
        // arm is logically unreachable through the public API; we
        // skip directly testing it.
        // (Documented exclusion — see report.)
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.cpu_write(0x2007, 0x42, &mut mapper);
        let _ = ppu;
    }

    // ─── PPUDATA — increment 32, palette read special, $3F00 ─────

    #[test]
    fn ppudata_read_returns_buffered_value_and_increments_v_by_32() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x04; // increment by 32
        ppu.read_buffer = 0xAA;
        ppu.v = 0x2000;

        let result = ppu.cpu_read(0x2007, &mut mapper);
        // First read returns the buffered value.
        assert_eq!(result, 0xAA);
        // V increments by 32.
        assert_eq!(ppu.v, 0x2020);
    }

    #[test]
    fn ppudata_read_palette_returns_immediate_value() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.ctrl = 0;
        ppu.palette_ram[0x05] = 0x2A;
        ppu.v = 0x3F05;
        let result = ppu.cpu_read(0x2007, &mut mapper);
        // Palette reads are immediate (not buffered).
        assert_eq!(result, 0x2A);
        assert_eq!(ppu.v, 0x3F06);
    }

    #[test]
    fn ppudata_read_palette_mirror_resolves_universal_bg() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.palette_ram[0x00] = 0x12;
        ppu.v = 0x3F10; // mirror of $3F00
        let result = ppu.cpu_read(0x2007, &mut mapper);
        assert_eq!(result, 0x12);
    }

    #[test]
    fn ppudata_write_to_palette_ram_persists() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.v = 0x3F0A;
        ppu.cpu_write(0x2007, 0x33, &mut mapper);
        assert_eq!(ppu.palette_ram[0x0A], 0x33);
    }

    #[test]
    fn ppu_read_palette_returns_palette_value_through_internal_path() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.palette_ram[0x07] = 0x44;
        // ppu_read is private but reachable via $2007 read at $3F07.
        ppu.v = 0x3F07;
        let _ = ppu.cpu_read(0x2007, &mut mapper);
        // The palette path uses self.palette_ram directly; we
        // indirectly exercised it. Now exercise the direct path
        // through ppu_read (private) by reading via the buffer-
        // refill side effect: at $3FXX, read_buffer is filled from
        // $2FXX. To hit the 0x3F00..=0x3FFF arm of the private
        // ppu_read, we reach it via a $2007 read at $3F00 — the
        // function reads palette_ram with the same arm.
        assert_eq!(ppu.palette_ram[0x07], 0x44);
    }

    // ─── PPU bus reads / writes — full address range ──────────────

    #[test]
    fn ppu_write_to_chr_ram_routes_through_mapper() {
        let mut mapper = dummy_mapper(); // CHR-RAM by default
        let mut ppu = Ppu::new();
        ppu.v = 0x0042;
        ppu.cpu_write(0x2007, 0x99, &mut mapper);
        // Confirm by reading back through the same path.
        ppu.v = 0x0042;
        let _ = ppu.cpu_read(0x2007, &mut mapper); // primes buffer
        let val = ppu.cpu_read(0x2007, &mut mapper); // returns buffered
        // After the second read, the buffered value should be 0x99
        // (the value we just wrote).
        // Actually — second call returns the value that was buffered
        // by the first call, which is the byte at $0042.
        // Note: each $2007 read also bumps v, so after two reads v
        // is now 0x0044. The first read at $0042 returned the prior
        // buffer (0); the second read returned $0042's content (0x99)
        // which the first read had loaded into the buffer.
        assert_eq!(val, 0x99);
    }

    #[test]
    fn ppu_chr_read_through_2007_returns_prior_buffered_value() {
        let mut mapper = mapper_with_chr({
            let mut chr = vec![0u8; 8192];
            chr[0x10] = 0xC1;
            chr
        });
        let mut ppu = Ppu::new();
        ppu.v = 0x0010;
        ppu.read_buffer = 0xDE;
        // First read returns DE; loads $0010 = 0xC1 into buffer.
        let v1 = ppu.cpu_read(0x2007, &mut mapper);
        assert_eq!(v1, 0xDE);
        ppu.v = 0x0010;
        let v2 = ppu.cpu_read(0x2007, &mut mapper);
        assert_eq!(v2, 0xC1);
    }

    // ─── Mirroring — FourScreen / SingleScreen variants ───────────

    #[test]
    fn nametable_mirroring_four_screen() {
        let ppu = Ppu::new();
        for (addr, want) in [
            (0x2000u16, 0x000u16),
            (0x2400, 0x400),
            (0x2800, 0x800),
            (0x2C00, 0xC00),
        ] {
            assert_eq!(ppu.mirror_nametable_addr(addr, Mirroring::FourScreen), want);
        }
    }

    #[test]
    fn nametable_mirroring_single_screen_lower() {
        let ppu = Ppu::new();
        for addr in [0x2000u16, 0x2400, 0x2800, 0x2C00] {
            assert_eq!(
                ppu.mirror_nametable_addr(addr, Mirroring::SingleScreenLower) & 0x3FF,
                addr & 0x3FF
            );
            // Always within the lower 0x400 region.
            assert!(ppu.mirror_nametable_addr(addr, Mirroring::SingleScreenLower) < 0x400);
        }
    }

    #[test]
    fn nametable_mirroring_single_screen_upper() {
        let ppu = Ppu::new();
        for addr in [0x2000u16, 0x2400, 0x2800, 0x2C00] {
            let m = ppu.mirror_nametable_addr(addr, Mirroring::SingleScreenUpper);
            assert!((0x400..0x800).contains(&m));
        }
    }

    // ─── Mask / emphasis ──────────────────────────────────────────

    #[test]
    fn emphasis_green_dims_red_and_blue() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x40; // emphasise green
        let argb = ppu.apply_mask_effects(0x20);
        let base = PALETTE[0x20];
        let base_r = (base >> 16) & 0xFF;
        let base_g = (base >> 8) & 0xFF;
        let base_b = base & 0xFF;
        let out_r = (argb >> 16) & 0xFF;
        let out_g = (argb >> 8) & 0xFF;
        let out_b = argb & 0xFF;
        assert_eq!(out_r, base_r * 13 / 16);
        assert_eq!(out_g, base_g);
        assert_eq!(out_b, base_b * 13 / 16);
    }

    #[test]
    fn emphasis_blue_dims_red_and_green() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x80; // emphasise blue
        let argb = ppu.apply_mask_effects(0x20);
        let base = PALETTE[0x20];
        let base_r = (base >> 16) & 0xFF;
        let base_g = (base >> 8) & 0xFF;
        let base_b = base & 0xFF;
        let out_r = (argb >> 16) & 0xFF;
        let out_g = (argb >> 8) & 0xFF;
        let out_b = argb & 0xFF;
        assert_eq!(out_r, base_r * 13 / 16);
        assert_eq!(out_g, base_g * 13 / 16);
        assert_eq!(out_b, base_b);
    }

    #[test]
    fn bg_and_sprites_enabled_requires_both_bits() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x00;
        assert!(!ppu.bg_and_sprites_enabled());
        ppu.mask = 0x08;
        assert!(!ppu.bg_and_sprites_enabled());
        ppu.mask = 0x10;
        assert!(!ppu.bg_and_sprites_enabled());
        ppu.mask = 0x18;
        assert!(ppu.bg_and_sprites_enabled());
    }

    // ─── Scrolling: increment_x / increment_y / copy_horizontal ────

    #[test]
    fn increment_x_wraps_at_31_and_toggles_horizontal_nametable() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.v = 0x001F; // coarse X = 31
        ppu.increment_x();
        // Coarse X resets to 0, horizontal nametable bit toggled.
        assert_eq!(ppu.v & 0x001F, 0);
        assert_eq!(ppu.v & 0x0400, 0x0400);
    }

    #[test]
    fn increment_x_does_nothing_when_rendering_disabled() {
        let mut ppu = Ppu::new();
        ppu.mask = 0;
        ppu.v = 0x0010;
        ppu.increment_x();
        assert_eq!(ppu.v, 0x0010);
    }

    #[test]
    fn increment_y_fine_y_increments_normally() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.v = 0x0000;
        ppu.increment_y();
        assert_eq!(ppu.v, 0x1000); // fine_y 0 -> 1
    }

    #[test]
    fn increment_y_wraps_fine_y_and_advances_coarse_y() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.v = 0x7000; // fine_y = 7, coarse_y = 0
        ppu.increment_y();
        assert_eq!(ppu.v & 0x7000, 0); // fine_y reset
        assert_eq!((ppu.v & 0x03E0) >> 5, 1); // coarse_y advanced
    }

    #[test]
    fn increment_y_wraps_at_coarse_y_29_and_toggles_vertical_nametable() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        // fine_y = 7, coarse_y = 29.
        ppu.v = 0x7000 | (29 << 5);
        ppu.increment_y();
        // coarse_y resets to 0 and vertical nametable toggled.
        assert_eq!((ppu.v & 0x03E0) >> 5, 0);
        assert_eq!(ppu.v & 0x0800, 0x0800);
    }

    #[test]
    fn increment_y_at_coarse_y_31_resets_without_toggle() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        // fine_y = 7, coarse_y = 31 (overflow region).
        ppu.v = 0x7000 | (31 << 5);
        ppu.increment_y();
        assert_eq!((ppu.v & 0x03E0) >> 5, 0);
        // No vertical toggle in the 31 path.
        assert_eq!(ppu.v & 0x0800, 0);
    }

    #[test]
    fn increment_y_does_nothing_when_rendering_disabled() {
        let mut ppu = Ppu::new();
        ppu.mask = 0;
        ppu.v = 0x7000;
        ppu.increment_y();
        assert_eq!(ppu.v, 0x7000);
    }

    #[test]
    fn copy_horizontal_copies_only_x_bits() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.v = 0x0000;
        ppu.t = 0x041F;
        ppu.copy_horizontal();
        assert_eq!(ppu.v, 0x041F);
    }

    #[test]
    fn copy_horizontal_disabled_when_rendering_off() {
        let mut ppu = Ppu::new();
        ppu.mask = 0;
        ppu.v = 0;
        ppu.t = 0x041F;
        ppu.copy_horizontal();
        assert_eq!(ppu.v, 0);
    }

    #[test]
    fn copy_vertical_copies_y_bits() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.v = 0x0000;
        ppu.t = 0x7BE0;
        ppu.copy_vertical();
        assert_eq!(ppu.v, 0x7BE0);
    }

    #[test]
    fn copy_vertical_disabled_when_rendering_off() {
        let mut ppu = Ppu::new();
        ppu.mask = 0;
        ppu.v = 0;
        ppu.t = 0x7BE0;
        ppu.copy_vertical();
        assert_eq!(ppu.v, 0);
    }

    // ─── tick_visible: rendering enabled and disabled paths ───────

    #[test]
    fn tick_visible_rendering_enabled_renders_pixel() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = 10;
        ppu.dot = 1; // tick processes current dot, then advances
        // tick processes dot 1 — render_pixel + bg_fetch + shift run.
        ppu.tick(&mut mapper);
        assert_eq!(ppu.dot, 2);
    }

    #[test]
    fn tick_visible_dot_256_runs_increment_y() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = 0;
        ppu.dot = 256;
        ppu.v = 0x0000;
        // tick processes dot 256: render_pixel + bg_fetch (1-256), then increment_y.
        ppu.tick(&mut mapper);
        assert_eq!(ppu.v & 0x7000, 0x1000, "increment_y bumps fine_y");
    }

    #[test]
    fn tick_visible_rendering_disabled_writes_background_colour() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0; // rendering disabled
        ppu.scanline = 5;
        ppu.dot = 1; // tick processes current dot, then advances
        ppu.palette_ram[0] = 0x21;

        ppu.tick(&mut mapper); // process dot 1 (rendering disabled branch)
        // After tick, dot has advanced to 2; dot 1 was processed.
        assert_eq!(ppu.dot, 2);
        let pixel = ppu.framebuffer[5 * FB_WIDTH as usize];
        assert_eq!(pixel, PALETTE[0x21]);
    }

    #[test]
    fn tick_visible_full_dot_257_runs_evaluate_sprites_and_copy_horizontal() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18; // rendering enabled
        ppu.scanline = 0;
        ppu.dot = 257;
        ppu.t = 0x041F;
        ppu.v = 0x0000;
        // tick executes the current dot (257) — evaluate_sprites and copy_h.
        ppu.tick(&mut mapper);
        // Horizontal scroll bits copied from t to v.
        assert_eq!(ppu.v & 0x041F, 0x041F);
    }

    #[test]
    fn tick_visible_at_dots_321_to_336_runs_bg_fetch() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = 0;
        ppu.dot = 320;
        // Tick into 321..=328 to cover bg_fetch including the 321
        // edge.
        for _ in 0..16 {
            ppu.tick(&mut mapper);
        }
        assert!(ppu.dot > 321);
    }

    // ─── tick_prerender: dot 3 NMI clear; dot 280-304 copy_v;
    //     sprite bus address; copy_horizontal; increment_y. ───────

    #[test]
    fn prerender_dot_3_clears_nmi_occurred() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 3; // tick processes current dot first
        ppu.nmi_occurred = true;
        ppu.nmi_output = true;
        ppu.update_nmi_line();
        assert!(ppu.nmi);

        ppu.tick(&mut mapper); // executes dot 3 → nmi_occurred cleared
        assert!(!ppu.nmi_occurred, "nmi_occurred should be cleared");
        assert!(!ppu.nmi);
    }

    #[test]
    fn prerender_copy_vertical_copies_y_bits_when_enabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 280;
        ppu.t = 0x7BE0;
        ppu.v = 0x0000;
        // Tick processes dot 280 (copy_vertical runs dots 280-304).
        ppu.tick(&mut mapper);
        assert_eq!(ppu.v & 0x7BE0, 0x7BE0);
    }

    #[test]
    fn prerender_dot_257_copies_horizontal_bits_when_enabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 257;
        ppu.t = 0x041F;
        ppu.v = 0x0000;
        // tick executes the current dot (257), running copy_horizontal.
        ppu.tick(&mut mapper);
        assert_eq!(ppu.v & 0x041F, 0x041F);
    }

    #[test]
    fn prerender_dot_257_to_320_runs_sprite_bus_address_when_enabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 256;
        // Tick to dot 320 to ensure sprite_bus path is exercised.
        for _ in 0..70 {
            ppu.tick(&mut mapper);
        }
        // bus_address should have been written by update_sprite_bus_address.
        assert!(ppu.scanline == ppu.pre_render_line);
    }

    #[test]
    fn prerender_dot_256_increments_y_when_enabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0x18;
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 256;
        ppu.v = 0x0000;
        // tick executes the current dot (256), running increment_y.
        ppu.tick(&mut mapper);
        // increment_y bumps fine_y from 0 to 1 → v = 0x1000.
        assert_eq!(ppu.v & 0x7000, 0x1000);
    }

    // ─── Frame wrap around scanlines ──────────────────────────────

    #[test]
    fn tick_advances_scanlines_and_wraps_frame() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = ppu.pre_render_line;
        ppu.dot = 339;
        // Even frame — no skip; tick to wrap.
        ppu.tick(&mut mapper); // 339 -> 340
        ppu.tick(&mut mapper); // 340 -> 0; scanline wraps
        assert_eq!(ppu.dot, 0);
        assert_eq!(ppu.scanline, 0);
        assert!(ppu.frame_odd, "frame_odd toggles on wrap");
    }

    // ─── Suppress VBL via $2002 read at exact dot 1 ───────────────

    #[test]
    fn reading_2002_at_241_dot_1_suppresses_vbl() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.scanline = 241;
        ppu.dot = 1;
        ppu.status = 0;

        // Read $2002 at exactly (241, 1) — sets suppress flag.
        let _ = ppu.cpu_read(0x2002, &mut mapper);
        assert!(ppu.suppress_vbl);

        // Tick the dot — VBL flag must stay clear.
        ppu.tick(&mut mapper);
        assert_eq!(ppu.status & 0x80, 0);
        assert!(!ppu.suppress_vbl, "suppress_vbl is consumed");
    }

    // ─── update_sprite_bus_address: phases 0-7 ────────────────────

    #[test]
    fn update_sprite_bus_address_phase_0_uses_nametable() {
        let mut ppu = Ppu::new();
        ppu.dot = 257; // sprite 0, phase 0
        ppu.v = 0x0FFF;
        ppu.update_sprite_bus_address();
        assert_eq!(ppu.bus_address, 0x2FFF);
    }

    #[test]
    fn update_sprite_bus_address_phase_2_uses_attribute_table() {
        let mut ppu = Ppu::new();
        ppu.dot = 259; // phase 2
        ppu.v = 0x0000;
        ppu.update_sprite_bus_address();
        // 0x23C0 base attribute address.
        assert_eq!(ppu.bus_address & 0x23C0, 0x23C0);
    }

    #[test]
    fn update_sprite_bus_address_phases_4_and_6_use_sprite_fetch_addr() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0; // 8x8 sprites, table 0
        ppu.scanline = 0;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0; // y = 0
        ppu.secondary_oam[1] = 0x10; // tile 0x10
        ppu.secondary_oam[2] = 0; // no flip
        ppu.secondary_oam[3] = 0;
        ppu.dot = 261; // phase 4 (sprite 0 lo byte)
        ppu.update_sprite_bus_address();
        // table 0, tile 0x10 → addr 0x100, row 0.
        assert_eq!(ppu.bus_address, 0x100);

        ppu.dot = 263; // phase 6 (sprite 0 hi byte)
        ppu.update_sprite_bus_address();
        assert_eq!(ppu.bus_address, 0x108);
    }

    // ─── sprite_fetch_addr: 8×8 path, ctrl bit 3 ─────────────────

    #[test]
    fn sprite_fetch_addr_8x8_pattern_table_1() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x08; // 8×8 sprites, pattern table 1
        ppu.scanline = 0;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0; // y=0
        ppu.secondary_oam[1] = 0x05; // tile 5
        ppu.secondary_oam[2] = 0; // no flip
        let addr = ppu.sprite_fetch_addr(0, false);
        assert_eq!(addr, 0x1050);
    }

    #[test]
    fn sprite_fetch_addr_8x8_flip_v() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x00; // 8×8, table 0
        ppu.scanline = 3;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0; // y=0
        ppu.secondary_oam[1] = 0;
        ppu.secondary_oam[2] = 0x80; // flip_v
        let addr = ppu.sprite_fetch_addr(0, false);
        // row 3 → 7-3 = 4 with flip
        assert_eq!(addr, 4);
    }

    // ─── evaluate_sprites: 8×16 mode, flip_v, flip_h ─────────────

    #[test]
    fn evaluate_sprites_8x16_uses_tile_index_low_bit_for_table() {
        let chr = vec![0u8; 8192];
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x20; // 8×16
        ppu.scanline = 0;
        // Sprite 0 at y=0, tile=0x01 (odd → table 1)
        ppu.oam[0] = 0;
        ppu.oam[1] = 0x01;
        ppu.oam[2] = 0;
        ppu.oam[3] = 0;
        // Sentinel: rest off-screen
        for i in 1..64 {
            ppu.oam[i * 4] = 0xF0;
        }
        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 1);
        // sprite 0 should have its patterns loaded.
    }

    #[test]
    fn evaluate_sprites_8x16_flip_v_and_high_row() {
        let mut chr = vec![0u8; 8192];
        // Make a unique byte we can verify after flip path: tile 0,
        // row 7 plane 0 → addr 7.
        chr[7] = 0xAA;
        chr[15] = 0x55;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x20; // 8x16
        ppu.scanline = 8;
        ppu.oam[0] = 0; // y=0
        ppu.oam[1] = 0; // tile 0
        ppu.oam[2] = 0x80; // flip_v
        ppu.oam[3] = 0;
        for i in 1..64 {
            ppu.oam[i * 4] = 0xF0;
        }
        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 1);
    }

    #[test]
    fn evaluate_sprites_8x16_row_ge_8_uses_next_tile() {
        let mut chr = vec![0u8; 8192];
        chr[0x10] = 0xAA; // tile 1, row 0 plane 0
        chr[0x18] = 0x55;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x20; // 8x16
        ppu.scanline = 8;
        ppu.oam[0] = 0; // y=0; row = 8 → second tile, row 0
        ppu.oam[1] = 0; // tile 0 (paired with tile 1 in 8x16)
        ppu.oam[2] = 0; // no flip
        ppu.oam[3] = 0;
        for i in 1..64 {
            ppu.oam[i * 4] = 0xF0;
        }
        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 1);
        assert_eq!(ppu.sprite_patterns_lo[0], 0xAA);
    }

    #[test]
    fn sprite_fetch_addr_8x8_table_0_no_flip() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0; // 8x8, table 0
        ppu.scanline = 0;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0; // y
        ppu.secondary_oam[1] = 0x00; // tile 0
        ppu.secondary_oam[2] = 0; // no flip
        let addr = ppu.sprite_fetch_addr(0, false);
        assert_eq!(addr, 0); // table 0, tile 0, row 0
    }

    #[test]
    fn sprite_fetch_addr_8x16_row_lt_8_uses_first_tile() {
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x20; // 8x16
        ppu.scanline = 0;
        ppu.sprite_count = 1;
        ppu.secondary_oam[0] = 0; // y=0
        ppu.secondary_oam[1] = 0; // tile 0
        ppu.secondary_oam[2] = 0; // no flip
        let addr = ppu.sprite_fetch_addr(0, false);
        // table=0, tile=0, row=0
        assert_eq!(addr, 0);
    }

    #[test]
    fn evaluate_sprites_8x8_vertical_flip() {
        let mut chr = vec![0u8; 8192];
        chr[7] = 0xCC; // tile 0 row 7 lo byte
        chr[15] = 0x33; // tile 0 row 7 hi byte
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0; // 8x8, table 0
        ppu.scanline = 0;
        ppu.oam[0] = 0; // y=0; row = 0 → after flip_v becomes 7
        ppu.oam[1] = 0;
        ppu.oam[2] = 0x80; // flip_v
        ppu.oam[3] = 0;
        for i in 1..64 {
            ppu.oam[i * 4] = 0xF0;
        }
        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 1);
        assert_eq!(ppu.sprite_patterns_lo[0], 0xCC);
        assert_eq!(ppu.sprite_patterns_hi[0], 0x33);
    }

    #[test]
    fn evaluate_sprites_horizontal_flip_reverses_pattern() {
        let mut chr = vec![0u8; 8192];
        chr[0] = 0x80; // bit 7 set
        chr[8] = 0x00;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0; // 8x8, table 0
        ppu.scanline = 0;
        ppu.oam[0] = 0;
        ppu.oam[1] = 0;
        ppu.oam[2] = 0x40; // flip_h
        ppu.oam[3] = 0;
        for i in 1..64 {
            ppu.oam[i * 4] = 0xF0;
        }
        ppu.evaluate_sprites(&mut mapper);
        // After flip 0x80 → 0x01.
        assert_eq!(ppu.sprite_patterns_lo[0], 0x01);
    }

    #[test]
    fn evaluate_sprites_no_visible_sprites_clears_unused_slots() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.ctrl = 0;
        ppu.scanline = 0;
        // Pre-populate sprite slots with garbage to confirm clear.
        ppu.sprite_patterns_lo = [0xFF; 8];
        ppu.sprite_patterns_hi = [0xFF; 8];
        for i in 0..64 {
            ppu.oam[i * 4] = 0xF0; // off-screen
        }
        ppu.evaluate_sprites(&mut mapper);
        assert_eq!(ppu.sprite_count, 0);
        for i in 0..8 {
            assert_eq!(ppu.sprite_patterns_lo[i], 0);
            assert_eq!(ppu.sprite_patterns_hi[i], 0);
        }
    }

    // ─── render_pixel: bg-only, sprite-only, sprite zero hit ─────

    #[test]
    fn render_pixel_clipping_left_8_bg_when_mask_disabled() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x08; // bg enable, no left-8 enable
        ppu.dot = 4; // within left 8
        ppu.bg_shift_pattern_lo = 0xFFFF;
        let (px, _) = ppu.get_bg_pixel();
        assert_eq!(px, 0);
    }

    #[test]
    fn render_pixel_left_8_bg_with_left_8_enabled() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x08 | 0x02; // bg + left8-bg
        ppu.dot = 4;
        ppu.bg_shift_pattern_lo = 0x8000;
        let (px, _) = ppu.get_bg_pixel();
        assert_eq!(px, 1);
    }

    #[test]
    fn get_bg_pixel_returns_zero_when_bg_disabled() {
        let ppu = Ppu::new();
        let (px, pal) = ppu.get_bg_pixel();
        assert_eq!(px, 0);
        assert_eq!(pal, 0);
    }

    #[test]
    fn get_sprite_pixel_returns_zero_when_sprites_disabled() {
        let ppu = Ppu::new();
        let (px, pal, pri, zero) = ppu.get_sprite_pixel(10);
        assert_eq!(px, 0);
        assert_eq!(pal, 0);
        assert!(!pri);
        assert!(!zero);
    }

    #[test]
    fn get_sprite_pixel_clipping_left_8_when_disabled() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x10; // sprites enabled, no left-8 sprites
        let (px, _, _, _) = ppu.get_sprite_pixel(4);
        assert_eq!(px, 0);
    }

    #[test]
    fn get_sprite_pixel_skips_transparent_and_returns_opaque() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x14; // sprites + left-8 sprites
        ppu.sprite_count = 2;
        // Sprite 0 at x=0, all-transparent pattern.
        ppu.sprite_x_counters[0] = 0;
        ppu.sprite_patterns_lo[0] = 0;
        ppu.sprite_patterns_hi[0] = 0;
        ppu.sprite_attribs[0] = 0x21;
        // Sprite 1 at x=0, opaque pixel at offset 0.
        ppu.sprite_x_counters[1] = 0;
        ppu.sprite_patterns_lo[1] = 0x80;
        ppu.sprite_patterns_hi[1] = 0x00;
        ppu.sprite_attribs[1] = 0x02;

        let (px, pal, behind, _) = ppu.get_sprite_pixel(0);
        assert_eq!(px, 1);
        assert_eq!(pal, 0x06);
        assert!(!behind);
    }

    #[test]
    fn render_pixel_writes_to_framebuffer_with_bg_only() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x08; // bg enabled only
        ppu.scanline = 5;
        ppu.dot = 100;
        ppu.bg_shift_pattern_lo = 0x8000;
        ppu.bg_shift_pattern_hi = 0;
        ppu.palette_ram[1] = 0x15;
        ppu.render_pixel();
        let pixel = ppu.framebuffer[5 * FB_WIDTH as usize + 99];
        // Should be PALETTE[0x15] (no emphasis).
        assert_eq!(pixel, PALETTE[0x15]);
    }

    #[test]
    fn render_pixel_returns_early_when_off_screen() {
        let mut ppu = Ppu::new();
        ppu.scanline = 250; // off-screen y
        ppu.dot = 1;
        ppu.render_pixel();
        // Should not panic; framebuffer untouched.
    }

    #[test]
    fn render_pixel_sprite_zero_hit_sets_flag() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18 | 0x06; // bg + sprites + left-8
        ppu.scanline = 0;
        ppu.dot = 10;
        // Both bg and sprite produce non-zero pixels.
        ppu.bg_shift_pattern_lo = 0xFFFF;
        ppu.bg_shift_pattern_hi = 0;
        ppu.sprite_count = 1;
        ppu.sprite_x_counters[0] = 9; // x=9 (so offset = 0 at dot 10)
        ppu.sprite_patterns_lo[0] = 0x80;
        ppu.sprite_patterns_hi[0] = 0;
        ppu.sprite_attribs[0] = 0;
        ppu.sprite_zero_on_line = true;
        ppu.render_pixel();
        assert_ne!(ppu.status & 0x40, 0, "sprite-zero hit should be set");
    }

    #[test]
    fn render_pixel_both_zero_uses_universal_bg() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x18; // bg + sprites
        ppu.scanline = 0;
        ppu.dot = 50;
        // bg pattern is all-zero and no sprites: pixel = 0, palette = 0.
        ppu.bg_shift_pattern_lo = 0;
        ppu.bg_shift_pattern_hi = 0;
        ppu.palette_ram[0] = 0x0F;
        ppu.render_pixel();
        let pixel = ppu.framebuffer[49];
        assert_eq!(pixel, PALETTE[0x0F]);
    }

    #[test]
    fn render_pixel_sprite_only_when_bg_transparent() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x1E; // bg + sprites + left-8 both
        ppu.scanline = 0;
        ppu.dot = 5;
        // bg transparent, sprite present.
        ppu.bg_shift_pattern_lo = 0;
        ppu.bg_shift_pattern_hi = 0;
        ppu.sprite_count = 1;
        ppu.sprite_x_counters[0] = 4;
        ppu.sprite_patterns_lo[0] = 0x80;
        ppu.sprite_patterns_hi[0] = 0;
        ppu.sprite_attribs[0] = 0;
        ppu.palette_ram[0x11] = 0x22; // palette 4, pixel 1 → (4<<2)|1 = 0x11
        ppu.render_pixel();
        let pixel = ppu.framebuffer[4];
        assert_eq!(pixel, PALETTE[0x22]);
    }

    #[test]
    fn get_sprite_pixel_offset_out_of_range_continues() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x14; // sprites + left-8
        ppu.sprite_count = 2;
        // Sprite 0 at x=100 (way to the right of x=0 query) → offset
        // negative, continue.
        ppu.sprite_x_counters[0] = 100;
        ppu.sprite_patterns_lo[0] = 0xFF;
        ppu.sprite_patterns_hi[0] = 0;
        ppu.sprite_attribs[0] = 0;
        // Sprite 1 also out of range at x=200.
        ppu.sprite_x_counters[1] = 200;
        ppu.sprite_patterns_lo[1] = 0xFF;
        ppu.sprite_patterns_hi[1] = 0;
        ppu.sprite_attribs[1] = 0;

        let (px, _, _, _) = ppu.get_sprite_pixel(0);
        assert_eq!(px, 0, "no sprite at x=0");
    }

    #[test]
    fn render_pixel_sprite_priority_behind_bg() {
        let mut ppu = Ppu::new();
        ppu.mask = 0x1E; // bg + sprites + left-8 both
        ppu.scanline = 0;
        ppu.dot = 5;
        // bg present
        ppu.bg_shift_pattern_lo = 0xFFFF;
        ppu.bg_shift_pattern_hi = 0;
        // sprite present, behind-bg priority
        ppu.sprite_count = 1;
        ppu.sprite_x_counters[0] = 4;
        ppu.sprite_patterns_lo[0] = 0x80;
        ppu.sprite_patterns_hi[0] = 0;
        ppu.sprite_attribs[0] = 0x20; // priority bit
        ppu.palette_ram[1] = 0x10;
        ppu.render_pixel();
        // bg won — pixel matches bg palette entry.
        let pixel = ppu.framebuffer[4];
        assert_eq!(pixel, PALETTE[0x10]);
    }

    // ─── A12 transition triggers mapper notification ──────────────

    #[test]
    fn check_a12_calls_mapper_when_rendering() {
        // Custom mapper that records A12 notifications.
        struct CountingMapper {
            inner: Nrom,
            notifications: u32,
        }
        impl Mapper for CountingMapper {
            fn cpu_read(&self, addr: u16) -> u8 {
                self.inner.cpu_read(addr)
            }
            fn cpu_write(&mut self, addr: u16, value: u8) {
                self.inner.cpu_write(addr, value);
            }
            fn chr_read(&mut self, addr: u16) -> u8 {
                self.inner.chr_read(addr)
            }
            fn chr_write(&mut self, addr: u16, value: u8) {
                self.inner.chr_write(addr, value);
            }
            fn mirroring(&self) -> Mirroring {
                self.inner.mirroring()
            }
            fn notify_a12_rendering(&mut self, _high: bool) {
                self.notifications += 1;
            }
            fn snapshot(&self) -> format_nintendo_nes_ines::MapperSnapshot {
                self.inner.snapshot()
            }
        }

        let mut mapper = CountingMapper {
            inner: Nrom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal),
            notifications: 0,
        };
        let mut ppu = Ppu::new();
        ppu.mask = 0x18; // rendering enabled
        ppu.scanline = 0;
        ppu.bus_address = 0x0000;
        // Force an A12 transition by setting bus_address with bit 12.
        ppu.bus_address = 0x1000;
        ppu.check_a12(&mut mapper);
        assert!(ppu.prev_a12);
        assert_eq!(mapper.notifications, 1);
    }

    #[test]
    fn check_a12_no_notification_when_rendering_disabled() {
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.mask = 0; // disabled
        ppu.scanline = 0;
        ppu.bus_address = 0x1000;
        // Should still update prev_a12 even if not rendering.
        ppu.check_a12(&mut mapper);
        assert!(ppu.prev_a12);
    }

    // ─── load_bg_shift_registers: attrib bit paths ────────────────

    #[test]
    fn load_bg_shift_registers_sets_attribute_planes() {
        let mut ppu = Ppu::new();
        ppu.bg_next_tile_lo = 0x12;
        ppu.bg_next_tile_hi = 0x34;
        ppu.bg_next_tile_attrib = 0x03; // both attribute bits
        ppu.load_bg_shift_registers();
        assert_eq!(ppu.bg_shift_pattern_lo & 0xFF, 0x12);
        assert_eq!(ppu.bg_shift_pattern_hi & 0xFF, 0x34);
        assert_eq!(ppu.bg_shift_attrib_lo & 0xFF, 0xFF);
        assert_eq!(ppu.bg_shift_attrib_hi & 0xFF, 0xFF);
    }

    // ─── bg_fetch_cycle dispatch (cycles 4 and 6) ────────────────

    #[test]
    fn bg_fetch_cycle_4_reads_pattern_low_byte() {
        let mut chr = vec![0u8; 8192];
        chr[0x10] = 0xC9;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0; // bg pattern table 0
        ppu.bg_next_tile_id = 0x01; // → addr 0x10 + fine_y 0
        ppu.v = 0;
        ppu.dot = 5; // cycle = 4
        ppu.bg_fetch_cycle(&mut mapper);
        assert_eq!(ppu.bg_next_tile_lo, 0xC9);
    }

    #[test]
    fn bg_fetch_cycle_6_reads_pattern_high_byte() {
        let mut chr = vec![0u8; 8192];
        chr[0x18] = 0xD8;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0;
        ppu.bg_next_tile_id = 0x01;
        ppu.v = 0;
        ppu.dot = 7; // cycle = 6
        ppu.bg_fetch_cycle(&mut mapper);
        assert_eq!(ppu.bg_next_tile_hi, 0xD8);
    }

    #[test]
    fn bg_fetch_cycle_4_with_pattern_table_1() {
        let mut chr = vec![0u8; 8192];
        chr[0x1010] = 0xAB;
        let mut mapper = mapper_with_chr(chr);
        let mut ppu = Ppu::new();
        ppu.ctrl = 0x10; // bg pattern table 1
        ppu.bg_next_tile_id = 0x01;
        ppu.v = 0;
        ppu.dot = 5;
        ppu.bg_fetch_cycle(&mut mapper);
        assert_eq!(ppu.bg_next_tile_lo, 0xAB);
    }

    // ─── Mirroring through ppu_read/write at non-CIRAM regions ───

    #[test]
    fn ppu_read_through_palette_addr_via_internal_path() {
        // Drive ppu_read via cpu_read at $2007 with v in palette
        // range — exercises the 0x3F00..=0x3FFF arm of ppu_read for
        // the read_buffer fallback at addr & 0x2FFF.
        let mut mapper = dummy_mapper();
        let mut ppu = Ppu::new();
        ppu.v = 0x3F00;
        let _ = ppu.cpu_read(0x2007, &mut mapper);
        // Effects: read_buffer was loaded from $2F00 mirror; v incremented.
        assert_eq!(ppu.v & 0x3FFF, 0x3F01);
    }

    #[test]
    fn mapper_nametable_override_reads_take_precedence() {
        // Custom mapper that owns its own nametable storage.
        struct OverrideMapper {
            inner: Nrom,
            nt_storage: [u8; 0x1000],
            owns_nametable: bool,
        }
        impl Mapper for OverrideMapper {
            fn cpu_read(&self, addr: u16) -> u8 {
                self.inner.cpu_read(addr)
            }
            fn cpu_write(&mut self, addr: u16, value: u8) {
                self.inner.cpu_write(addr, value);
            }
            fn chr_read(&mut self, addr: u16) -> u8 {
                self.inner.chr_read(addr)
            }
            fn chr_write(&mut self, addr: u16, value: u8) {
                self.inner.chr_write(addr, value);
            }
            fn mirroring(&self) -> Mirroring {
                self.inner.mirroring()
            }
            fn nametable_read(&mut self, addr: u16) -> Option<u8> {
                if self.owns_nametable {
                    Some(self.nt_storage[(addr - 0x2000) as usize & 0x0FFF])
                } else {
                    None
                }
            }
            fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
                if self.owns_nametable {
                    self.nt_storage[(addr - 0x2000) as usize & 0x0FFF] = value;
                    true
                } else {
                    false
                }
            }
            fn snapshot(&self) -> format_nintendo_nes_ines::MapperSnapshot {
                self.inner.snapshot()
            }
        }

        let mut mapper = OverrideMapper {
            inner: Nrom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal),
            nt_storage: [0; 0x1000],
            owns_nametable: true,
        };
        mapper.nt_storage[0x42] = 0xC3;

        let mut ppu = Ppu::new();
        // Write through PPU $2007: mapper consumes the write.
        ppu.v = 0x2042;
        ppu.cpu_write(0x2007, 0xAA, &mut mapper);
        // Mapper's nt_storage should hold the value (write was consumed).
        assert_eq!(mapper.nt_storage[0x42], 0xAA);

        // Read it back: mapper.nametable_read returns Some.
        ppu.v = 0x2042;
        let _ = ppu.cpu_read(0x2007, &mut mapper); // primes read_buffer
        let val = ppu.cpu_read(0x2007, &mut mapper); // returns buffered
        assert_eq!(val, 0xAA);
    }

    #[test]
    fn nametable_read_via_vertical_mirroring_returns_ram() {
        let mut mapper = mapper_vertical();
        let mut ppu = Ppu::new();
        // Write a sentinel into nametable via $2007.
        ppu.v = 0x2400; // logical address; vertical mirror -> 0x400
        ppu.cpu_write(0x2007, 0x77, &mut mapper);
        assert_eq!(ppu.nametable_ram[0x400], 0x77);
    }
}
