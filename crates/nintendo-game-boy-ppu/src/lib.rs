//! Game Boy DMG PPU.
//!
//! Pixel-FIFO renderer ticked at the master clock rate (one call per
//! T-cycle / dot). The 4-state BG/window fetcher reads tile IDs and
//! pattern data from VRAM, decodes 8 pixels per fetch into a 16-slot
//! FIFO, and emits one shade per pixel per dot once the FIFO has
//! data. OAM scan happens once at the mode-2 → mode-3 transition,
//! limited to 10 sprites per scanline and sorted by X for DMG
//! priority.
//!
//! Framebuffer holds post-palette 2-bit shade values (0 = lightest,
//! 3 = darkest); the runtime layer maps those to RGBA via the
//! [`common-nintendo-game-boy::palette`] helpers (or a custom green-LCD
//! palette).
//!
//! Ported from `~/Projects/Emu198x-Zig/src/ppu.zig`. The pin contract
//! with the machine is two one-shot pulses — `vblank_irq_latched`
//! and `stat_irq_latched` — that the machine consumes via
//! [`Ppu::consume_vblank_irq`] / [`Ppu::consume_stat_irq`] and OR's
//! into `IF` bits 0 / 1.

mod fetcher;
mod fifo;
mod sprite;

use common_nintendo_game_boy::{SCREEN_HEIGHT, SCREEN_WIDTH};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::fetcher::{FetchCtx, Fetcher};
use crate::fifo::Fifo;
use crate::sprite::Sprite;

/// MMIO addresses for the PPU register block.
pub const REG_LCDC: u16 = 0xFF40;
pub const REG_STAT: u16 = 0xFF41;
pub const REG_SCY: u16 = 0xFF42;
pub const REG_SCX: u16 = 0xFF43;
pub const REG_LY: u16 = 0xFF44;
pub const REG_LYC: u16 = 0xFF45;
pub const REG_BGP: u16 = 0xFF47;
pub const REG_OBP0: u16 = 0xFF48;
pub const REG_OBP1: u16 = 0xFF49;
pub const REG_WY: u16 = 0xFF4A;
pub const REG_WX: u16 = 0xFF4B;

/// `IF` bit positions latched by the PPU.
pub const IF_VBLANK_BIT: u8 = 0;
pub const IF_STAT_BIT: u8 = 1;

const DOTS_PER_LINE: u16 = 456;
const LINES_PER_FRAME: u8 = 154;
const VBLANK_START: u8 = 144;
const OAM_END: u16 = 80;
const LCD_ENABLE_MODE0_DOTS: u16 = 80;

const FRAMEBUFFER_LEN: usize = (SCREEN_WIDTH * SCREEN_HEIGHT) as usize;

const fn default_lyc_match() -> bool {
    true
}

/// LCDC bit positions. The fetcher reaches into LCDC via raw bit
/// constants so it doesn't need to share this module; the names
/// here document the full bit field for the lib-side dispatch.
#[allow(dead_code)]
mod lcdc {
    pub const ENABLE: u8 = 0x80;
    pub const WINDOW_TILE_MAP: u8 = 0x40;
    pub const WINDOW_ENABLE: u8 = 0x20;
    pub const BG_TILE_DATA_UNSIGNED: u8 = 0x10;
    pub const BG_TILE_MAP: u8 = 0x08;
    pub const SPRITE_HEIGHT_16: u8 = 0x04;
    pub const SPRITES_ENABLE: u8 = 0x02;
    pub const BG_ENABLE: u8 = 0x01;
}

/// STAT bit positions for the writable interrupt-enable bits.
mod stat {
    pub const LYC_ENABLE: u8 = 0x40;
    pub const MODE2_ENABLE: u8 = 0x20;
    pub const MODE1_ENABLE: u8 = 0x10;
    pub const MODE0_ENABLE: u8 = 0x08;
    /// Mask covering the writable bits 3-6.
    pub const WRITABLE_MASK: u8 = 0x78;
}

/// PPU state.
#[derive(Clone, Serialize, Deserialize)]
pub struct Ppu {
    /// Dot counter within the current scanline (0..=455).
    pub dot: u16,
    /// Current scanline (0..=153).
    pub ly: u8,

    /// X coordinate of the next pixel to emit (0..=160).
    pub lcd_x: u8,
    /// Pixels at the start of mode 3 to discard for SCX fine-scroll
    /// alignment (= SCX & 7).
    discard_pixels: u8,

    fifo: Fifo,
    fetcher: Fetcher,

    sprites: [Sprite; 10],
    sprite_count: u8,

    /// Internal scanline counter that only advances on lines where
    /// the window was actually drawn — independent of the BG SCY.
    window_line: u8,
    /// `true` if the window triggered on the current scanline.
    window_triggered: bool,

    pub lcdc: u8,
    /// STAT's three writable interrupt-enable bits + the LYC-enable
    /// bit. Mode bits are computed on read; LYC coincidence is latched
    /// because LCD-off behaviour does not simply mirror `LY == LYC`.
    stat: u8,
    #[serde(default = "default_lyc_match")]
    lyc_match: bool,
    #[serde(default)]
    lcd_enable_mode0_dots: u16,
    pub scy: u8,
    pub scx: u8,
    pub lyc: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wx: u8,
    pub wy: u8,

    #[serde(with = "BigArray")]
    framebuffer: [u8; FRAMEBUFFER_LEN],
    /// Set when the PPU enters VBlank; the runtime consumes this to
    /// know a frame is ready to present.
    pub frame_ready: bool,

    /// One-shot VBlank IRQ pulse, cleared by
    /// [`consume_vblank_irq`](Ppu::consume_vblank_irq).
    vblank_irq_latched: bool,
    /// One-shot STAT IRQ pulse, cleared by
    /// [`consume_stat_irq`](Ppu::consume_stat_irq). Asserted on the
    /// rising edge of the OR of the four STAT enable sources.
    stat_irq_latched: bool,
    /// Last-known STAT-line state; used for rising-edge detection.
    stat_line_prev: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    /// Creates a PPU at the documented post-boot-ROM register state
    /// for the DMG (LCD on, BG enabled, BGP = $FC, OBP0/1 = $FF).
    #[must_use]
    pub fn new() -> Self {
        Self {
            dot: 0,
            ly: 0,
            lcd_x: 0,
            discard_pixels: 0,
            fifo: Fifo::new(),
            fetcher: Fetcher::new(),
            sprites: [Sprite::EMPTY; 10],
            sprite_count: 0,
            window_line: 0,
            window_triggered: false,
            lcdc: 0x91,
            stat: 0,
            lyc_match: true,
            lcd_enable_mode0_dots: 0,
            scy: 0,
            scx: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wx: 0,
            wy: 0,
            framebuffer: [0; FRAMEBUFFER_LEN],
            frame_ready: false,
            vblank_irq_latched: false,
            stat_irq_latched: false,
            stat_line_prev: false,
        }
    }

    /// Returns the current PPU mode.
    ///
    /// | Mode | Meaning            |
    /// |------|--------------------|
    /// | 0    | HBlank             |
    /// | 1    | VBlank             |
    /// | 2    | OAM scan           |
    /// | 3    | Pixel transfer     |
    #[must_use]
    pub fn mode(&self) -> u8 {
        if (self.lcdc & lcdc::ENABLE) == 0 || self.lcd_enable_mode0_dots != 0 {
            0
        } else if self.ly >= VBLANK_START {
            1
        } else if self.dot < OAM_END {
            2
        } else if self.dot < self.mode3_end_dot() || self.lcd_x < SCREEN_WIDTH as u8 {
            3
        } else {
            0
        }
    }

    fn mode3_end_dot(&self) -> u16 {
        OAM_END + 172 + u16::from(self.scx & 7) + self.obj_mode3_penalty()
    }

    fn obj_mode3_penalty(&self) -> u16 {
        if (self.lcdc & lcdc::SPRITES_ENABLE) == 0 {
            return 0;
        }

        let mut seen_bg_tiles = [false; 32];
        let mut penalty = 0u16;
        for sprite in self.sprites.iter().take(usize::from(self.sprite_count)) {
            if sprite.x == 0 {
                penalty += 11;
                continue;
            }

            let screen_x = sprite.x.saturating_sub(8);
            if screen_x >= SCREEN_WIDTH as u8 {
                continue;
            }

            let bg_x = u16::from(self.scx) + u16::from(screen_x);
            let tile = usize::from((bg_x / 8) & 0x1F);
            if !seen_bg_tiles[tile] {
                seen_bg_tiles[tile] = true;
                let pixels_to_right = 7 - (bg_x & 7);
                penalty += pixels_to_right.saturating_sub(2);
            }

            penalty += 6;
        }

        penalty
    }

    /// Reads STAT ($FF41). Composes the writable bits with the live
    /// mode and LYC coincidence flag.
    #[must_use]
    pub fn read_stat(&self) -> u8 {
        let mut s = self.stat & stat::WRITABLE_MASK;
        s |= self.mode();
        if self.effective_lyc_match() {
            s |= 0x04;
        }
        s
    }

    /// Reads LY ($FF44). Near the end of a visible scanline, the CPU
    /// observes the next line before the internal dot counter wraps.
    #[must_use]
    pub fn read_ly(&self) -> u8 {
        if self.cpu_visible_next_ly() {
            self.ly.wrapping_add(1)
        } else {
            self.ly
        }
    }

    /// Returns whether CPU reads from VRAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_vram_read(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && self.lcd_enable_mode0_dots == 0
            && self.dot >= OAM_END - 4
            && self.dot < self.mode3_end_dot()
    }

    /// Returns whether CPU writes to VRAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_vram_write(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && self.dot >= OAM_END
            && self.dot < self.mode3_end_dot()
    }

    /// Returns whether CPU reads from OAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_oam_read(&self) -> bool {
        matches!(self.mode(), 2 | 3) || self.cpu_visible_next_ly()
    }

    /// Returns whether CPU writes to OAM are blocked by the PPU.
    #[must_use]
    pub fn cpu_blocks_oam_write(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0
            && self.ly < VBLANK_START
            && ((self.lcd_enable_mode0_dots == 0 && self.dot < OAM_END - 4)
                || (self.dot >= OAM_END && self.dot < self.mode3_end_dot()))
    }

    fn effective_lyc_match(&self) -> bool {
        if self.cpu_visible_next_ly() {
            false
        } else {
            self.lyc_match
        }
    }

    fn cpu_visible_next_ly(&self) -> bool {
        (self.lcdc & lcdc::ENABLE) != 0 && self.ly < VBLANK_START && self.dot >= 452
    }

    /// Writes STAT ($FF41). Only bits 3-6 are writable.
    pub fn write_stat(&mut self, value: u8) {
        self.stat = value & stat::WRITABLE_MASK;
        // A write that newly enables a STAT source can immediately
        // raise the line — re-evaluate edge detection.
        self.update_stat_line();
    }

    /// Writes LYC ($FF45) and re-evaluates the coincidence STAT
    /// source against the current LY.
    pub fn write_lyc(&mut self, value: u8) {
        self.lyc = value;
        if (self.lcdc & lcdc::ENABLE) != 0 {
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line();
    }

    /// Writes LCDC ($FF40). Turning the LCD off freezes timing,
    /// resets the line counter, and blanks the framebuffer (real
    /// hardware shows white). Turning it back on resumes from
    /// `dot = 0, ly = 0`.
    pub fn write_lcdc(&mut self, value: u8) {
        let was_on = (self.lcdc & lcdc::ENABLE) != 0;
        let now_on = (value & lcdc::ENABLE) != 0;
        self.lcdc = value;
        if was_on && !now_on {
            self.dot = 0;
            self.ly = 0;
            self.lcd_x = 0;
            self.window_line = 0;
            self.window_triggered = false;
            self.fifo.clear();
            self.fetcher.reset();
            self.framebuffer.fill(0);
            self.stat_line_prev = false;
        } else if !was_on && now_on {
            self.lcd_enable_mode0_dots = LCD_ENABLE_MODE0_DOTS;
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line();
    }

    /// Reads the framebuffer as a flat `width * height` slice of 2-bit
    /// shades (0 = lightest, 3 = darkest). The runtime maps to RGBA.
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Consumes the frame-ready latch: returns `true` and clears the
    /// flag if the PPU has entered VBlank since the last call.
    pub fn consume_frame_ready(&mut self) -> bool {
        let was = self.frame_ready;
        self.frame_ready = false;
        was
    }

    /// Consumes the VBlank IRQ pulse. The machine OR's the result
    /// into `IF` bit 0.
    pub fn consume_vblank_irq(&mut self) -> bool {
        let was = self.vblank_irq_latched;
        self.vblank_irq_latched = false;
        was
    }

    /// Consumes the STAT IRQ pulse. The machine OR's the result into
    /// `IF` bit 1.
    pub fn consume_stat_irq(&mut self) -> bool {
        let was = self.stat_irq_latched;
        self.stat_irq_latched = false;
        was
    }

    /// Advance the PPU by one T-cycle (one dot).
    ///
    /// `vram` must be the 8 KiB DMG VRAM block (i.e. CPU `$8000`
    /// is `vram[0]`). `oam` must be the 160-byte OAM block (CPU
    /// `$FE00` = `oam[0]`).
    pub fn tick(&mut self, vram: &[u8], oam: &[u8]) {
        if (self.lcdc & lcdc::ENABLE) == 0 {
            // LCD off: timing frozen.
            return;
        }

        if self.ly >= VBLANK_START {
            // Mode 1: VBlank. The single-shot frame-ready + vblank
            // IRQ latch fire on the very first dot of line 144.
            if self.ly == VBLANK_START && self.dot == 0 {
                self.frame_ready = true;
                self.vblank_irq_latched = true;
            }
        } else if self.dot < OAM_END {
            // Mode 2: OAM scan. We do the scan once at the
            // mode-2 → mode-3 transition (after the CPU has had a
            // chance to handle STAT interrupts that might change
            // LCDC).
        } else {
            if self.dot == OAM_END {
                self.scan_oam(vram, oam);
                self.fetcher.reset();
                self.fifo.clear();
                self.lcd_x = 0;
                self.discard_pixels = self.scx & 7;
                self.window_triggered = false;
            }

            if self.lcd_x < SCREEN_WIDTH as u8 {
                // Window trigger check (once per pixel position).
                if !self.fetcher.is_window()
                    && (self.lcdc & lcdc::WINDOW_ENABLE) != 0
                    && self.ly >= self.wy
                    && self.lcd_x.wrapping_add(7) >= self.wx
                {
                    self.fetcher.switch_to_window();
                    self.fifo.clear();
                    self.window_triggered = true;
                }

                // Mode 3: pixel transfer.
                let ctx = FetchCtx {
                    lcdc: self.lcdc,
                    ly: self.ly,
                    scx: self.scx,
                    scy: self.scy,
                    window_line: self.window_line,
                    _marker: core::marker::PhantomData,
                };
                self.fetcher.tick(ctx, &mut self.fifo, vram);

                if self.fifo.len() > 0 {
                    let bg_index = self.fifo.pop();
                    if self.discard_pixels > 0 {
                        self.discard_pixels -= 1;
                    } else {
                        // BG/window disabled on DMG forces colour 0.
                        let effective_index = if (self.lcdc & lcdc::BG_ENABLE) != 0 {
                            bg_index
                        } else {
                            0
                        };

                        let mut final_shade = apply_palette(self.bgp, effective_index);
                        if (self.lcdc & lcdc::SPRITES_ENABLE) != 0
                            && let Some(sprite_shade) =
                                self.sprite_pixel(self.lcd_x, effective_index)
                        {
                            final_shade = sprite_shade;
                        }

                        let pixel_idx =
                            usize::from(self.ly) * SCREEN_WIDTH as usize + usize::from(self.lcd_x);
                        self.framebuffer[pixel_idx] = final_shade;
                        self.lcd_x += 1;
                    }
                }
            }
            // else: Mode 0 (HBlank) — idle until next scanline.
        }

        self.advance_timing();
        self.update_stat_line();
    }

    /// Tick four times — one CPU m-cycle.
    pub fn tick_m(&mut self, vram: &[u8], oam: &[u8]) {
        for _ in 0..4 {
            self.tick(vram, oam);
        }
    }

    fn advance_timing(&mut self) {
        self.lcd_enable_mode0_dots = self.lcd_enable_mode0_dots.saturating_sub(1);
        self.dot += 1;
        if self.dot >= DOTS_PER_LINE {
            self.dot = 0;
            if self.window_triggered {
                self.window_line = self.window_line.wrapping_add(1);
            }
            self.ly = self.ly.wrapping_add(1);
            if self.ly >= LINES_PER_FRAME {
                self.ly = 0;
                self.window_line = 0;
            }
            self.lyc_match = self.ly == self.lyc;
            if self.ly < VBLANK_START {
                self.lcd_x = 0;
            }
        }
    }

    /// Composite the current STAT IRQ source line and latch a pulse
    /// on its rising edge.
    fn update_stat_line(&mut self) {
        let mode = self.mode();
        let line = ((self.stat & stat::LYC_ENABLE) != 0 && self.effective_lyc_match())
            || ((self.stat & stat::MODE2_ENABLE) != 0 && mode == 2)
            || ((self.stat & stat::MODE1_ENABLE) != 0 && mode == 1)
            || ((self.stat & stat::MODE0_ENABLE) != 0 && mode == 0);
        if line && !self.stat_line_prev {
            self.stat_irq_latched = true;
        }
        self.stat_line_prev = line;
    }

    /// Scan OAM for sprites visible on the current scanline. Decodes
    /// each visible sprite's pixel row and applies DMG priority
    /// sorting (lower X wins, OAM order is the tiebreaker).
    fn scan_oam(&mut self, vram: &[u8], oam: &[u8]) {
        self.sprite_count = 0;
        let height: u8 = if (self.lcdc & lcdc::SPRITE_HEIGHT_16) != 0 {
            16
        } else {
            8
        };

        for i in 0..40 {
            if self.sprite_count >= 10 {
                break;
            }
            let oam_y = oam[i * 4];
            let oam_x = oam[i * 4 + 1];
            let tile = oam[i * 4 + 2];
            let attr = oam[i * 4 + 3];

            let screen_y = i16::from(oam_y) - 16;
            let ly_signed = i16::from(self.ly);
            if ly_signed < screen_y || ly_signed >= screen_y + i16::from(height) {
                continue;
            }

            let mut row = (ly_signed - screen_y) as u8;
            if (attr & 0x40) != 0 {
                row = height - 1 - row; // Y flip
            }

            // 8x16 sprites: lower bit of tile ignored, second tile = tile+1.
            let mut tile_id = tile;
            if height == 16 {
                tile_id &= 0xFE;
                if row >= 8 {
                    tile_id |= 0x01;
                    row -= 8;
                }
            }

            // Sprites always use $8000 unsigned addressing.
            let tile_addr = u16::from(tile_id) * 16 + u16::from(row) * 2;
            let low = vram[usize::from(tile_addr)];
            let high = vram[usize::from(tile_addr + 1)];

            let mut sprite = Sprite {
                y: screen_y.max(0) as u8,
                x: oam_x,
                tile: tile_id,
                attr,
                pixels: [0; 8],
            };

            for px in 0..8u8 {
                let bit = if (attr & 0x20) != 0 {
                    px // X flip: bit 0 is leftmost
                } else {
                    7 - px
                };
                let l = (low >> bit) & 1;
                let h = (high >> bit) & 1;
                sprite.pixels[usize::from(px)] = (h << 1) | l;
            }

            self.sprites[usize::from(self.sprite_count)] = sprite;
            self.sprite_count += 1;
        }

        // Insertion sort by X — stable, so OAM order survives ties.
        let mut j = 1u8;
        while j < self.sprite_count {
            let key = self.sprites[usize::from(j)];
            let mut k = j;
            while k > 0 && self.sprites[usize::from(k - 1)].x > key.x {
                self.sprites[usize::from(k)] = self.sprites[usize::from(k - 1)];
                k -= 1;
            }
            self.sprites[usize::from(k)] = key;
            j += 1;
        }
    }

    /// Composite a sprite pixel at `(lcd_x, ly)` if any visible sprite
    /// covers it. Applies BG-priority and palette selection per the
    /// sprite's attribute byte.
    fn sprite_pixel(&self, lcd_x: u8, bg_index: u8) -> Option<u8> {
        for i in 0..usize::from(self.sprite_count) {
            let s = &self.sprites[i];
            // Sprite OAM x is screen_x + 8; the sprite covers
            // [x-8, x). Use wrapping math so off-screen sprites
            // (x < 8 or x > 168) compare correctly.
            let lcd_plus_8 = u16::from(lcd_x) + 8;
            let sprite_x = u16::from(s.x);
            if lcd_plus_8 < sprite_x || lcd_plus_8 >= sprite_x + 8 {
                continue;
            }
            let px_in_sprite = (lcd_plus_8 - sprite_x) as usize;
            let sprite_index = s.pixels[px_in_sprite];
            if sprite_index == 0 {
                continue; // transparent
            }

            // BG-priority bit: when set and BG index != 0, BG wins.
            if (s.attr & 0x80) != 0 && bg_index != 0 {
                continue;
            }

            let palette = if (s.attr & 0x10) != 0 {
                self.obp1
            } else {
                self.obp0
            };
            return Some(apply_palette(palette, sprite_index));
        }
        None
    }
}

/// Apply a 2-bit BGP/OBP palette to a 2-bit pixel index.
#[inline]
pub fn apply_palette(palette: u8, index: u8) -> u8 {
    (palette >> ((index & 0b11) * 2)) & 0b11
}

#[cfg(test)]
mod tests;
