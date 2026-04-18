//! MOS 6569 PAL / 6567 NTSC VIC-II video chip.
//!
//! The VIC-II is the C64's video chip. It drives the dot clock, owns
//! video memory reads, renders text / bitmap / sprites to an ARGB
//! framebuffer, steals CPU cycles during bad lines and sprite DMA,
//! and generates raster / collision / light-pen interrupts.
//!
//! Each [`Vic::tick`] advances one `phi2` cycle and renders 8 pixels.
//! This first fresh-workspace port keeps the archived crate's proven
//! raster, badline, sprite-BA, IRQ, and display-mode behaviour.

#![allow(clippy::cast_possible_truncation)]

pub mod palette;

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use palette::PALETTE;

const PAL_FIRST_VISIBLE_LINE: u16 = 0;
const PAL_LAST_VISIBLE_LINE: u16 = 312;
const NTSC_FIRST_VISIBLE_LINE: u16 = 14;
const NTSC_LAST_VISIBLE_LINE: u16 = 258;
const FIRST_VISIBLE_CYCLE: u8 = 10;
const LAST_VISIBLE_CYCLE: u8 = 62;
const VISIBLE_CYCLES: u8 = LAST_VISIBLE_CYCLE - FIRST_VISIBLE_CYCLE;
pub const FB_WIDTH: u32 = VISIBLE_CYCLES as u32 * 8;
pub const FB_HEIGHT: u32 = (PAL_LAST_VISIBLE_LINE - PAL_FIRST_VISIBLE_LINE) as u32;
const DISPLAY_START_LINE: u16 = 0x30;
const DISPLAY_END_LINE: u16 = 0xF8;
const DISPLAY_START_CYCLE: u8 = 16;
const DISPLAY_END_CYCLE: u8 = 56;
const SPRITE_X_TO_FB: i16 = 24;

/// Narrow VIC-visible memory bus.
pub trait VicMemory {
    /// Read a byte from VIC-visible memory using the full 16-bit VIC address.
    fn read_vram(&self, addr: u16) -> u8;

    /// Read a colour RAM nibble at the given 0-1023 offset.
    fn read_colour(&self, offset: u16) -> u8;
}

/// VIC-II model variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VicModel {
    /// PAL 6569: 312 lines, 63 cycles per line.
    #[default]
    Pal6569,
    /// NTSC 6567: 263 lines, 65 cycles per line.
    Ntsc6567,
}

impl VicModel {
    /// Total raster lines per frame.
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Pal6569 => 312,
            Self::Ntsc6567 => 263,
        }
    }

    /// `phi2` cycles per raster line.
    #[must_use]
    pub const fn cycles_per_line(self) -> u8 {
        match self {
            Self::Pal6569 => 63,
            Self::Ntsc6567 => 65,
        }
    }
}

struct CellPixels {
    colour: [u32; 8],
    fg_mask: u8,
}

impl CellPixels {
    fn solid(c: u32) -> Self {
        Self {
            colour: [c; 8],
            fg_mask: 0,
        }
    }
}

/// VIC-II chip state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vic {
    /// IRQ output pin.
    pub irq: bool,
    /// BA pin, represented as `true` when BA is asserted low.
    pub ba_low: bool,

    #[serde(with = "BigArray")]
    regs: [u8; 0x40],
    raster_line: u16,
    raster_cycle: u8,
    raster_compare: u16,
    irq_status: u8,
    irq_enable: u8,
    is_badline: bool,
    den_latch: bool,
    frame_complete: bool,
    framebuffer: Vec<u32>,
    #[serde(with = "BigArray")]
    screen_row: [u8; 40],
    #[serde(with = "BigArray")]
    colour_row: [u8; 40],
    char_row: u8,
    vic_bank: u8,
    sprite_data: [[u8; 3]; 8],
    sprite_active: [bool; 8],
    sprite_dma_active: [bool; 8],
    sprite_sprite_collision: u8,
    sprite_bg_collision: u8,
    sprite_sprite_irq_latched: bool,
    sprite_bg_irq_latched: bool,
    text_row: u16,
    xscroll_carry_pixels: [u32; 8],
    xscroll_carry_fg: u8,
    xscroll_latch: u8,
    lines_per_frame: u16,
    cycles_per_line: u8,
    first_visible_line: u16,
    last_visible_line: u16,
    lp_triggered: bool,
    last_bus_data: u8,
}

impl Vic {
    /// Construct a VIC-II for the given hardware model.
    #[must_use]
    pub fn new(model: VicModel) -> Self {
        let (first_vis, last_vis) = match model {
            VicModel::Pal6569 => (PAL_FIRST_VISIBLE_LINE, PAL_LAST_VISIBLE_LINE),
            VicModel::Ntsc6567 => (NTSC_FIRST_VISIBLE_LINE, NTSC_LAST_VISIBLE_LINE),
        };
        let visible_lines = u32::from(last_vis - first_vis);
        let fb_size = FB_WIDTH as usize * visible_lines as usize;

        Self {
            irq: false,
            ba_low: false,
            regs: [0; 0x40],
            raster_line: 0,
            raster_cycle: 0,
            raster_compare: 0,
            irq_status: 0,
            irq_enable: 0,
            is_badline: false,
            den_latch: false,
            frame_complete: false,
            framebuffer: vec![0xFF00_0000; fb_size],
            screen_row: [0; 40],
            colour_row: [0; 40],
            char_row: 0,
            vic_bank: 0,
            sprite_data: [[0; 3]; 8],
            sprite_active: [false; 8],
            sprite_dma_active: [false; 8],
            sprite_sprite_collision: 0,
            sprite_bg_collision: 0,
            sprite_sprite_irq_latched: false,
            sprite_bg_irq_latched: false,
            text_row: 0,
            xscroll_carry_pixels: [0; 8],
            xscroll_carry_fg: 0,
            xscroll_latch: 0,
            lines_per_frame: model.lines_per_frame(),
            cycles_per_line: model.cycles_per_line(),
            first_visible_line: first_vis,
            last_visible_line: last_vis,
            lp_triggered: false,
            last_bus_data: 0,
        }
    }

    /// Tick the VIC-II for one `phi2` cycle.
    pub fn tick(&mut self, memory: &dyn VicMemory) -> bool {
        if self.raster_cycle == 55 {
            self.evaluate_sprite_dma();
        }

        if self.raster_cycle == 0
            && self.raster_line >= self.first_visible_line
            && self.raster_line < self.last_visible_line
        {
            self.fetch_sprite_data(memory);
        }

        self.render_pixels(memory);
        self.check_badline();

        let badline_stall = self.is_badline && (15..=54).contains(&self.raster_cycle);
        let sprite_stall = self.is_sprite_dma_stealing();
        let cpu_stalled = badline_stall || sprite_stall;
        self.ba_low = self.compute_ba_low();

        if self.is_badline && self.raster_cycle == 15 {
            self.char_row = 0;
            self.fetch_screen_row(memory);
        }

        self.raster_cycle += 1;
        if self.raster_cycle >= self.cycles_per_line {
            self.raster_cycle = 0;
            self.raster_line += 1;

            if self.raster_line >= self.lines_per_frame {
                self.raster_line = 0;
                self.frame_complete = true;
                self.den_latch = false;
                self.lp_triggered = false;
            }

            if self.den_latch && (DISPLAY_START_LINE..0xFBu16).contains(&self.raster_line) {
                self.char_row = (self.char_row + 1) & 7;
            }
        }

        if self.raster_line == self.raster_compare && self.raster_cycle == 0 {
            self.irq_status |= 0x01;
        }

        self.irq = (self.irq_status & self.irq_enable & 0x0F) != 0;
        cpu_stalled
    }

    fn vram_addr(&self, bank_offset: u16) -> u16 {
        u16::from(self.vic_bank) * 0x4000 + (bank_offset & 0x3FFF)
    }

    fn check_badline(&mut self) {
        let den = self.regs[0x11] & 0x10 != 0;
        let yscroll = u16::from(self.regs[0x11] & 0x07);

        if self.raster_line == DISPLAY_START_LINE && den {
            self.den_latch = true;
        }

        self.is_badline = self.den_latch
            && (DISPLAY_START_LINE..DISPLAY_END_LINE).contains(&self.raster_line)
            && (self.raster_line & 7) == yscroll;
    }

    fn fetch_screen_row(&mut self, memory: &dyn VicMemory) {
        let screen_base = self.screen_base();
        let text_row = (self.raster_line - DISPLAY_START_LINE) / 8;
        self.text_row = text_row;

        for col in 0u16..40 {
            let screen_addr = screen_base + text_row * 40 + col;
            let byte = memory.read_vram(self.vram_addr(screen_addr));
            self.screen_row[col as usize] = byte;
            self.last_bus_data = byte;
            self.colour_row[col as usize] = memory.read_colour(text_row * 40 + col);
        }
    }

    fn fetch_sprite_data(&mut self, memory: &dyn VicMemory) {
        let sprite_enable = self.regs[0x15];
        let y_expand = self.regs[0x17];
        let screen_base = self.screen_base();

        for i in 0..8usize {
            self.sprite_active[i] = false;

            if sprite_enable & (1 << i) == 0 {
                continue;
            }

            let sprite_y = u16::from(self.regs[1 + i * 2]);
            let height = if y_expand & (1 << i) != 0 {
                42u16
            } else {
                21u16
            };
            let line_in_sprite = self.raster_line.wrapping_sub(sprite_y);
            if line_in_sprite >= height {
                continue;
            }

            let data_line = if y_expand & (1 << i) != 0 {
                line_in_sprite / 2
            } else {
                line_in_sprite
            };

            let ptr_addr = screen_base + 0x03F8 + i as u16;
            let sprite_ptr = memory.read_vram(self.vram_addr(ptr_addr));
            self.last_bus_data = sprite_ptr;

            let data_base = u16::from(sprite_ptr) * 64 + data_line * 3;
            self.sprite_data[i][0] = memory.read_vram(self.vram_addr(data_base));
            self.sprite_data[i][1] = memory.read_vram(self.vram_addr(data_base + 1));
            self.sprite_data[i][2] = memory.read_vram(self.vram_addr(data_base + 2));
            self.last_bus_data = self.sprite_data[i][2];
            self.sprite_active[i] = true;
        }
    }

    fn render_pixels(&mut self, memory: &dyn VicMemory) {
        if self.raster_line < self.first_visible_line || self.raster_line >= self.last_visible_line
        {
            return;
        }
        if self.raster_cycle < FIRST_VISIBLE_CYCLE || self.raster_cycle >= LAST_VISIBLE_CYCLE {
            return;
        }

        let fb_y = (self.raster_line - self.first_visible_line) as usize;
        let fb_x = (self.raster_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        let fb_offset = fb_y * FB_WIDTH as usize + fb_x;
        let border_colour = PALETTE[(self.regs[0x20] & 0x0F) as usize];
        let rsel = self.regs[0x11] & 0x08 != 0;
        let char_vstart = if rsel { 0x33u16 } else { 0x37u16 };
        let char_vstop = if rsel { 0xFBu16 } else { 0xF7u16 };
        let in_char_area = self.den_latch
            && (char_vstart..char_vstop).contains(&self.raster_line)
            && (DISPLAY_START_CYCLE..DISPLAY_END_CYCLE).contains(&self.raster_cycle);

        let mut fg_mask: u8 = 0;

        if self.raster_cycle == DISPLAY_START_CYCLE && in_char_area {
            self.xscroll_latch = self.regs[0x16] & 0x07;
            let bg = PALETTE[(self.regs[0x21] & 0x0F) as usize];
            self.xscroll_carry_pixels = [bg; 8];
            self.xscroll_carry_fg = 0;
        }

        if in_char_area {
            let display_cycle = self.raster_cycle - DISPLAY_START_CYCLE;
            let col = display_cycle as usize;

            if col < 40 {
                let char_code = self.screen_row[col];
                let colour_nybble = self.colour_row[col];
                let bmm = self.regs[0x11] & 0x20 != 0;
                let ecm = self.regs[0x11] & 0x40 != 0;
                let mcm = self.regs[0x16] & 0x10 != 0;

                let cell = if ecm && (bmm || mcm) {
                    CellPixels::solid(PALETTE[0])
                } else if bmm && mcm {
                    self.render_mcm_bitmap(col, char_code, colour_nybble, memory)
                } else if bmm {
                    self.render_hires_bitmap(col, char_code, memory)
                } else if ecm {
                    self.render_ecm_text(char_code, colour_nybble, memory)
                } else if mcm {
                    self.render_mcm_text(char_code, colour_nybble, memory)
                } else {
                    self.render_standard_text(char_code, colour_nybble, memory)
                };

                let xscroll = self.xscroll_latch as usize;

                if xscroll == 0 {
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = cell.colour[px];
                        }
                    }
                    fg_mask = cell.fg_mask;
                } else {
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            if px < xscroll {
                                self.framebuffer[idx] = self.xscroll_carry_pixels[px];
                                if (self.xscroll_carry_fg >> px) & 1 != 0 {
                                    fg_mask |= 1 << px;
                                }
                            } else {
                                self.framebuffer[idx] = cell.colour[px - xscroll];
                                if (cell.fg_mask >> (px - xscroll)) & 1 != 0 {
                                    fg_mask |= 1 << px;
                                }
                            }
                        }
                    }
                    for i in 0..xscroll {
                        self.xscroll_carry_pixels[i] = cell.colour[8 - xscroll + i];
                    }
                    self.xscroll_carry_fg =
                        (cell.fg_mask >> (8 - xscroll)) & ((1u8 << xscroll) - 1);
                }
            }
        }

        let csel = self.regs[0x16] & 0x08 != 0;
        let vstart = if rsel { 0x33u16 } else { 0x37u16 };
        let vstop = if rsel { 0xFBu16 } else { 0xF7u16 };
        let hstart = if csel {
            DISPLAY_START_CYCLE
        } else {
            DISPLAY_START_CYCLE + 1
        };
        let hstop = if csel {
            DISPLAY_END_CYCLE
        } else {
            DISPLAY_END_CYCLE - 1
        };
        let in_visible_window = self.den_latch
            && (vstart..vstop).contains(&self.raster_line)
            && (hstart..hstop).contains(&self.raster_cycle);

        if !in_visible_window {
            for px in 0..8usize {
                let idx = fb_offset + px;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = border_colour;
                }
            }
            fg_mask = 0;
        }

        self.overlay_sprites(fb_offset, fb_x, fg_mask);

        if self.sprite_sprite_collision != 0 && !self.sprite_sprite_irq_latched {
            self.sprite_sprite_irq_latched = true;
            self.irq_status |= 0x04;
        }
        if self.sprite_bg_collision != 0 && !self.sprite_bg_irq_latched {
            self.sprite_bg_irq_latched = true;
            self.irq_status |= 0x02;
        }
    }

    fn render_standard_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg_colour = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];
        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_hires_bitmap(&self, col: usize, char_code: u8, memory: &dyn VicMemory) -> CellPixels {
        let fg_colour = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let bg_colour = PALETTE[(char_code & 0x0F) as usize];
        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_ecm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg_select = (char_code >> 6) & 0x03;
        let bg_colour = PALETTE[(self.regs[0x21 + bg_select as usize] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];
        let char_base = self.char_base();
        let effective_char = char_code & 0x3F;
        let bitmap_addr = char_base + u16::from(effective_char) * 8 + u16::from(self.char_row);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            if bit != 0 {
                cell.fg_mask |= 1 << px;
                cell.colour[px] = fg_colour;
            } else {
                cell.colour[px] = bg_colour;
            }
        }
        cell
    }

    fn render_mcm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        if colour_nybble & 0x08 == 0 {
            return self.render_standard_text(char_code, colour_nybble, memory);
        }

        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let bg1 = PALETTE[(self.regs[0x22] & 0x0F) as usize];
        let bg2 = PALETTE[(self.regs[0x23] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x07) as usize];
        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for pair in 0..4usize {
            let bits = (bitmap >> (6 - pair * 2)) & 0x03;
            let colour = match bits {
                0b00 => bg0,
                0b01 => bg1,
                0b10 => bg2,
                _ => fg_colour,
            };
            let is_fg = bits != 0b00;
            let px0 = pair * 2;
            let px1 = px0 + 1;
            if is_fg {
                cell.fg_mask |= (1 << px0) | (1 << px1);
            }
            cell.colour[px0] = colour;
            cell.colour[px1] = colour;
        }
        cell
    }

    fn render_mcm_bitmap(
        &self,
        col: usize,
        char_code: u8,
        colour_nybble: u8,
        memory: &dyn VicMemory,
    ) -> CellPixels {
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let c01 = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let c10 = PALETTE[(char_code & 0x0F) as usize];
        let c11 = PALETTE[(colour_nybble & 0x0F) as usize];
        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = memory.read_vram(self.vram_addr(bitmap_addr));

        let mut cell = CellPixels {
            colour: [0; 8],
            fg_mask: 0,
        };
        for pair in 0..4usize {
            let bits = (bitmap >> (6 - pair * 2)) & 0x03;
            let colour = match bits {
                0b00 => bg0,
                0b01 => c01,
                0b10 => c10,
                _ => c11,
            };
            let is_fg = bits != 0b00;
            let px0 = pair * 2;
            let px1 = px0 + 1;
            if is_fg {
                cell.fg_mask |= (1 << px0) | (1 << px1);
            }
            cell.colour[px0] = colour;
            cell.colour[px1] = colour;
        }
        cell
    }

    fn overlay_sprites(&mut self, fb_offset: usize, fb_x_start: usize, fg_mask: u8) {
        let priority = self.regs[0x1B];
        let x_expand = self.regs[0x1D];
        let mcm_reg = self.regs[0x1C];
        let mc0 = PALETTE[(self.regs[0x25] & 0x0F) as usize];
        let mc1 = PALETTE[(self.regs[0x26] & 0x0F) as usize];
        let mut sprite_coverage: [u8; 8] = [0; 8];
        let mut sprite_colour: [[u32; 8]; 8] = [[0; 8]; 8];

        for i in 0..8usize {
            if !self.sprite_active[i] {
                continue;
            }

            let sprite_x = u16::from(self.regs[i * 2])
                | if self.regs[0x10] & (1 << i) != 0 {
                    256
                } else {
                    0
                };
            let expanded_x = x_expand & (1 << i) != 0;
            let is_mcm = mcm_reg & (1 << i) != 0;
            let sprite_col = PALETTE[(self.regs[0x27 + i] & 0x0F) as usize];
            let sprite_fb_x = i16::try_from(sprite_x).unwrap_or(0) + SPRITE_X_TO_FB;
            let sprite_width: i16 = if expanded_x { 48 } else { 24 };

            for px in 0..8usize {
                let screen_px = fb_x_start as i16 + px as i16;
                let pixel_in_sprite = screen_px - sprite_fb_x;

                if pixel_in_sprite < 0 || pixel_in_sprite >= sprite_width {
                    continue;
                }

                let data_pos = if expanded_x {
                    pixel_in_sprite / 2
                } else {
                    pixel_in_sprite
                } as usize;

                if is_mcm {
                    let pair_idx = data_pos / 2;
                    let byte_idx = pair_idx / 4;
                    let shift = 6 - (pair_idx % 4) * 2;
                    let bits = (self.sprite_data[i][byte_idx] >> shift) & 0x03;

                    if bits != 0b00 {
                        sprite_coverage[px] |= 1 << i;
                        sprite_colour[px][i] = match bits {
                            0b01 => mc0,
                            0b10 => sprite_col,
                            _ => mc1,
                        };
                    }
                } else {
                    let byte_idx = data_pos / 8;
                    let bit_idx = 7 - (data_pos % 8);
                    if self.sprite_data[i][byte_idx] & (1 << bit_idx) != 0 {
                        sprite_coverage[px] |= 1 << i;
                        sprite_colour[px][i] = sprite_col;
                    }
                }
            }
        }

        for (px, &cov) in sprite_coverage.iter().enumerate() {
            if cov.count_ones() >= 2 {
                self.sprite_sprite_collision |= cov;
            }
            if cov != 0 && (fg_mask & (1 << px)) != 0 {
                self.sprite_bg_collision |= cov;
            }
        }

        for px in 0..8usize {
            let idx = fb_offset + px;
            if idx >= self.framebuffer.len() {
                continue;
            }

            for i in (0..8usize).rev() {
                if sprite_coverage[px] & (1 << i) == 0 {
                    continue;
                }

                let behind_fg = priority & (1 << i) != 0;
                if behind_fg && (fg_mask & (1 << px)) != 0 {
                    continue;
                }

                self.framebuffer[idx] = sprite_colour[px][i];
            }
        }
    }

    fn evaluate_sprite_dma(&mut self) {
        let sprite_enable = self.regs[0x15];
        let y_expand = self.regs[0x17];

        for i in 0..8usize {
            if sprite_enable & (1 << i) == 0 {
                self.sprite_dma_active[i] = false;
                continue;
            }

            let sprite_y = u16::from(self.regs[1 + i * 2]);
            let height = if y_expand & (1 << i) != 0 {
                42u16
            } else {
                21u16
            };
            let offset = self.raster_line.wrapping_sub(sprite_y);
            self.sprite_dma_active[i] = offset < height;
        }
    }

    fn is_sprite_dma_stealing(&self) -> bool {
        let c = self.raster_cycle;
        (self.sprite_dma_active[0] && (c == 58 || c == 59))
            || (self.sprite_dma_active[1] && (c == 60 || c == 61))
            || (self.sprite_dma_active[2] && (c == 62 || c == 0))
            || (self.sprite_dma_active[3] && (c == 1 || c == 2))
            || (self.sprite_dma_active[4] && (c == 3 || c == 4))
            || (self.sprite_dma_active[5] && (c == 5 || c == 6))
            || (self.sprite_dma_active[6] && (c == 7 || c == 8))
            || (self.sprite_dma_active[7] && (c == 9 || c == 10))
    }

    fn compute_ba_low(&self) -> bool {
        self.badline_ba_low() || self.sprite_ba_low()
    }

    fn badline_ba_low(&self) -> bool {
        self.is_badline && (12..=54).contains(&self.raster_cycle)
    }

    fn sprite_ba_low(&self) -> bool {
        let c = self.raster_cycle;
        let cpl = self.cycles_per_line;
        for i in 0..8u8 {
            if !self.sprite_dma_active[i as usize] {
                continue;
            }
            let ba_start = (55u8.wrapping_add(2 * i)) % cpl;
            let ba_end = (59u8.wrapping_add(2 * i)) % cpl;
            if ba_start <= ba_end {
                if c >= ba_start && c <= ba_end {
                    return true;
                }
            } else if c >= ba_start || c <= ba_end {
                return true;
            }
        }
        false
    }

    fn screen_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 4) & 0x0F) * 0x0400
    }

    fn char_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 1) & 0x07) * 0x0800
    }

    fn bitmap_base(&self) -> u16 {
        if self.regs[0x18] & 0x08 != 0 {
            0x2000
        } else {
            0x0000
        }
    }

    /// Read a VIC-II register.
    pub fn read(&mut self, reg: u8) -> u8 {
        // Per reference: $D019 bits 6:4 read as 1, $D01A bits 7:4 read
        // as 1, and unused bits on colour regs ($D020-$D02E) read as 1.
        match reg & 0x3F {
            0x11 => {
                (self.regs[0x11] & 0x7F)
                    | if self.raster_line & 0x100 != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x12 => (self.raster_line & 0xFF) as u8,
            0x19 => {
                let composite = if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.irq_status | composite | 0x70
            }
            0x1A => (self.irq_enable & 0x0F) | 0xF0,
            0x1E => {
                let val = self.sprite_sprite_collision;
                self.sprite_sprite_collision = 0;
                self.sprite_sprite_irq_latched = false;
                val
            }
            0x1F => {
                let val = self.sprite_bg_collision;
                self.sprite_bg_collision = 0;
                self.sprite_bg_irq_latched = false;
                val
            }
            r @ 0x20..=0x2E => self.regs[r as usize] | 0xF0,
            _ => self.last_bus_data,
        }
    }

    /// Read a register without side effects.
    #[must_use]
    pub fn peek(&self, reg: u8) -> u8 {
        match reg & 0x3F {
            0x11 => {
                (self.regs[0x11] & 0x7F)
                    | if self.raster_line & 0x100 != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x12 => (self.raster_line & 0xFF) as u8,
            // peek() returns the same composite IRR, but we keep the
            // raw peek semantics — callers that want the canonical
            // silicon-observable read mask should use read() instead.
            0x19 => {
                self.irq_status
                    | if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                        0x80
                    } else {
                        0x00
                    }
            }
            0x1A => self.irq_enable & 0x0F,
            0x1E => self.sprite_sprite_collision,
            0x1F => self.sprite_bg_collision,
            r if r <= 0x2E => self.regs[r as usize],
            _ => self.last_bus_data,
        }
    }

    /// Write a VIC-II register.
    pub fn write(&mut self, reg: u8, value: u8) {
        let r = (reg & 0x3F) as usize;
        if r < self.regs.len() {
            self.regs[r] = value;
        }

        match reg & 0x3F {
            0x11 => {
                self.raster_compare =
                    (self.raster_compare & 0x00FF) | (u16::from(value & 0x80) << 1);
            }
            0x12 => {
                self.raster_compare = (self.raster_compare & 0x0100) | u16::from(value);
            }
            0x19 => {
                self.irq_status &= !value & 0x0F;
            }
            0x1A => {
                self.irq_enable = value & 0x0F;
            }
            _ => {}
        }

        self.irq = (self.irq_status & self.irq_enable & 0x0F) != 0;
    }

    /// Whether the IRQ pin is asserted.
    #[must_use]
    pub const fn irq_active(&self) -> bool {
        self.irq
    }

    /// Whether BA is asserted low.
    #[must_use]
    pub const fn ba_is_low(&self) -> bool {
        self.ba_low
    }

    /// Set the active VIC bank.
    pub fn set_bank(&mut self, bank: u8) {
        self.vic_bank = bank & 0x03;
    }

    /// Current VIC bank.
    #[must_use]
    pub const fn bank(&self) -> u8 {
        self.vic_bank
    }

    /// Trigger the light-pen latch once per frame.
    pub fn trigger_light_pen(&mut self) {
        if self.lp_triggered {
            return;
        }
        self.lp_triggered = true;
        self.regs[0x13] = (u16::from(self.raster_cycle) * 4) as u8;
        self.regs[0x14] = self.raster_line as u8;
    }

    /// Borrow the ARGB32 framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub const fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        u32::from(self.last_visible_line - self.first_visible_line)
    }

    /// Check and clear the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let complete = self.frame_complete;
        self.frame_complete = false;
        complete
    }

    /// Current raster line.
    #[must_use]
    pub const fn raster_line(&self) -> u16 {
        self.raster_line
    }

    /// Current cycle within the raster line.
    #[must_use]
    pub const fn raster_cycle(&self) -> u8 {
        self.raster_cycle
    }

    /// Current character row within an 8-line character cell.
    #[must_use]
    pub const fn char_row(&self) -> u8 {
        self.char_row
    }

    /// Whether the current line is a bad line.
    #[must_use]
    pub const fn is_badline(&self) -> bool {
        self.is_badline
    }

    /// Borrow the raw register file.
    #[must_use]
    pub const fn registers(&self) -> &[u8; 0x40] {
        &self.regs
    }

    /// Restore the raw register file from saved state.
    pub fn set_registers(&mut self, regs: &[u8; 0x40]) {
        self.regs = *regs;
        self.raster_compare = u16::from(self.regs[0x12]) | (u16::from(self.regs[0x11] & 0x80) << 1);
        self.irq_enable = self.regs[0x1A] & 0x0F;
    }

    /// Snapshot of the IRQ status register.
    #[must_use]
    pub const fn irq_status(&self) -> u8 {
        self.irq_status
    }

    /// Restore the IRQ status register.
    pub fn set_irq_status(&mut self, val: u8) {
        self.irq_status = val;
    }
}

impl Default for Vic {
    fn default() -> Self {
        Self::new(VicModel::Pal6569)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES_PER_FRAME: u16 = 312;
    const CYCLES_PER_LINE: u8 = 63;
    const FIRST_VISIBLE_LINE: u16 = PAL_FIRST_VISIBLE_LINE;

    struct TestMemory {
        ram: Box<[u8; 0x10000]>,
        char_rom: Vec<u8>,
        colour_ram: Vec<u8>,
    }

    impl TestMemory {
        fn new(chargen: &[u8]) -> Self {
            Self {
                ram: Box::new([0; 0x10000]),
                char_rom: chargen.to_vec(),
                colour_ram: vec![0; 1024],
            }
        }

        fn with_colour(chargen: &[u8], colour_ram: Vec<u8>) -> Self {
            Self {
                ram: Box::new([0; 0x10000]),
                char_rom: chargen.to_vec(),
                colour_ram,
            }
        }

        fn ram_write(&mut self, addr: u16, value: u8) {
            self.ram[addr as usize] = value;
        }
    }

    impl VicMemory for TestMemory {
        fn read_vram(&self, addr: u16) -> u8 {
            let bank = (addr >> 14) & 0x03;
            let bank_addr = addr & 0x3FFF;
            if (bank == 0 || bank == 2) && (0x1000..0x2000).contains(&bank_addr) {
                self.char_rom[(bank_addr - 0x1000) as usize]
            } else {
                self.ram[addr as usize]
            }
        }

        fn read_colour(&self, offset: u16) -> u8 {
            self.colour_ram
                .get(offset as usize)
                .copied()
                .map(|v| v & 0x0F)
                .unwrap_or(0)
        }
    }

    fn make_vic_and_memory() -> (Vic, TestMemory) {
        let chargen = vec![0xFF; 4096];
        let vic = Vic::new(VicModel::Pal6569);
        let memory = TestMemory::new(&chargen);
        (vic, memory)
    }

    fn tick_vic(vic: &mut Vic, mem: &TestMemory) -> bool {
        vic.tick(mem)
    }

    fn advance_to(vic: &mut Vic, memory: &TestMemory, line: u16, cycle: u8) {
        let target = u32::from(line) * u32::from(CYCLES_PER_LINE) + u32::from(cycle);
        for _ in 0..target {
            tick_vic(vic, memory);
        }
    }

    fn fb_pixel(vic: &Vic, fb_x: usize, fb_y: usize) -> u32 {
        vic.framebuffer()[fb_y * FB_WIDTH as usize + fb_x]
    }

    #[test]
    fn initial_state() {
        let mut vic = Vic::new(VicModel::Pal6569);
        assert_eq!(vic.raster_line(), 0);
        assert_eq!(vic.raster_cycle(), 0);
        assert!(!vic.irq_active());
        assert!(!vic.take_frame_complete());
        assert!(!vic.irq);
        assert!(!vic.ba_low);
    }

    #[test]
    fn raster_advances() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..63 {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.raster_line(), 1);
        assert_eq!(vic.raster_cycle(), 0);
    }

    #[test]
    fn frame_complete_after_full_frame() {
        let (mut vic, memory) = make_vic_and_memory();
        let total_cycles = u32::from(LINES_PER_FRAME) * u32::from(CYCLES_PER_LINE);
        for _ in 0..total_cycles {
            tick_vic(&mut vic, &memory);
        }
        assert!(vic.take_frame_complete());
        assert!(!vic.take_frame_complete());
    }

    #[test]
    fn raster_irq_fires_and_acknowledges() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x12, 1);
        vic.write(0x1A, 0x01);

        for _ in 0..63 {
            tick_vic(&mut vic, &memory);
        }
        assert!(vic.irq_active());
        assert!(vic.irq);

        vic.write(0x19, 0x01);
        assert!(!vic.irq_active());
        assert!(!vic.irq);
    }

    #[test]
    fn framebuffer_size() {
        let vic = Vic::new(VicModel::Pal6569);
        assert_eq!(
            vic.framebuffer().len(),
            FB_WIDTH as usize * FB_HEIGHT as usize
        );
    }

    #[test]
    fn register_read_write() {
        // Colour registers $D020-$D02E: bits 7:4 read as 1 per reference,
        // so writing $06 reads back as $F6 and $01 reads back as $F1.
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x20, 0x06);
        assert_eq!(vic.read(0x20), 0xF6);
        vic.write(0x21, 0x01);
        assert_eq!(vic.read(0x21), 0xF1);
    }

    #[test]
    fn bank_selection_masks_to_two_bits() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.set_bank(2);
        assert_eq!(vic.bank(), 2);
        vic.set_bank(5);
        assert_eq!(vic.bank(), 1);
    }

    #[test]
    fn sprite_renders_at_correct_position() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x01);
        vic.write(0x00, 172);
        vic.write(0x01, 100);
        vic.write(0x27, 0x01);
        vic.write(0x18, 0x14);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);
        vic.write(0x11, 0x1B);

        let target_line = 100u16;
        let target_cycle = 35u8;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..cycles_to_target {
            tick_vic(&mut vic, &memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let idx = fb_y * FB_WIDTH as usize + 196;
        assert_eq!(vic.framebuffer()[idx], PALETTE[1]);
    }

    #[test]
    fn bitmap_base_selection() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x18, 0x14);
        assert_eq!(vic.bitmap_base(), 0x0000);
        vic.write(0x18, 0x1C);
        assert_eq!(vic.bitmap_base(), 0x2000);
    }

    #[test]
    fn collision_registers_clear_on_read() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.sprite_sprite_collision = 0x05;
        vic.sprite_bg_collision = 0x0A;
        assert_eq!(vic.read(0x1E), 0x05);
        assert_eq!(vic.read(0x1E), 0x00);
        assert_eq!(vic.read(0x1F), 0x0A);
        assert_eq!(vic.read(0x1F), 0x00);
    }

    #[test]
    fn collision_peek_does_not_clear() {
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.sprite_sprite_collision = 0x03;
        assert_eq!(vic.peek(0x1E), 0x03);
        assert_eq!(vic.peek(0x1E), 0x03);
        assert_eq!(vic.read(0x1E), 0x03);
        assert_eq!(vic.read(0x1E), 0x00);
    }

    #[test]
    fn sprite_sprite_collision_set_on_overlap() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x15, 0x03);
        vic.write(0x00, 172);
        vic.write(0x01, 100);
        vic.write(0x02, 172);
        vic.write(0x03, 100);
        vic.write(0x27, 0x01);
        vic.write(0x28, 0x02);
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        let target_line = 100u16;
        let target_cycle = 35u8;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }

        let collision = vic.read(0x1E);
        assert_eq!(collision & 0x03, 0x03);
        assert_eq!(vic.read(0x1E), 0x00);
    }

    #[test]
    fn sprite_bg_collision_set_on_foreground_overlap() {
        let chargen = vec![0xFF; 4096];
        let colour_ram = {
            let mut v = vec![0u8; 1024];
            v[0] = 0x01;
            v
        };
        let mut memory = TestMemory::with_colour(&chargen, colour_ram);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x15, 0x01);
        vic.write(0x11, 0x1B);
        vic.write(0x18, 0x14);
        vic.write(0x00, 24);
        vic.write(0x01, 51);
        vic.write(0x27, 0x01);
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        let target_line = 51u16;
        let target_cycle = DISPLAY_START_CYCLE + 1;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }

        let collision = vic.read(0x1F);
        assert_ne!(collision & 0x01, 0x00);
    }

    #[test]
    fn invalid_mode_renders_black() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x7B);
        vic.write(0x20, 0x06);
        vic.write(0x21, 0x01);

        let target_line = DISPLAY_START_LINE + 3;
        let target_cycle = DISPLAY_START_CYCLE + 5;
        let total = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=total {
            tick_vic(&mut vic, &memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x = (target_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        assert_eq!(fb_pixel(&vic, fb_x, fb_y), PALETTE[0]);
    }

    #[test]
    fn ecm_text_selects_background() {
        let chargen = vec![0x00; 4096];
        let memory = TestMemory::new(&chargen);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x11, 0x5B);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);
        vic.write(0x22, 0x02);
        vic.write(0x23, 0x05);
        vic.write(0x24, 0x06);

        let target_line = DISPLAY_START_LINE + 3;
        let past_fetch = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + 16;
        for _ in 0..past_fetch {
            tick_vic(&mut vic, &memory);
        }
        vic.screen_row[0] = 0x00;
        vic.screen_row[1] = 0x40;
        vic.screen_row[2] = 0x80;
        vic.screen_row[3] = 0xC0;

        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        assert_eq!(fb_pixel(&vic, fb_x0, fb_y), PALETTE[0]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 8, fb_y), PALETTE[2]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 16, fb_y), PALETTE[5]);

        tick_vic(&mut vic, &memory);
        assert_eq!(fb_pixel(&vic, fb_x0 + 24, fb_y), PALETTE[6]);
    }

    #[test]
    fn badline_ba_low_cycles_12_to_54() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        advance_to(&mut vic, &memory, 0x33, 0);

        for cycle in 0..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            let expected = (12..=54).contains(&cycle);
            assert_eq!(vic.ba_low, expected);
        }
    }

    #[test]
    fn non_badline_does_not_assert_ba() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        advance_to(&mut vic, &memory, 0x34, 0);

        for _ in 0..CYCLES_PER_LINE {
            tick_vic(&mut vic, &memory);
            assert!(!vic.badline_ba_low());
        }
    }

    #[test]
    fn sprite_ba_asserts_with_three_cycle_leadin() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x15, 0x01);
        vic.write(0x01, 0);
        advance_to(&mut vic, &memory, 0, 55);
        tick_vic(&mut vic, &memory);

        for cycle in 56..CYCLES_PER_LINE {
            let expected = (55..=59).contains(&cycle);
            assert_eq!(vic.sprite_ba_low(), expected);
            tick_vic(&mut vic, &memory);
        }
    }

    #[test]
    fn light_pen_latches_beam_position() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..20 {
            tick_vic(&mut vic, &memory);
        }
        let cycle = vic.raster_cycle();
        let line = vic.raster_line();
        vic.trigger_light_pen();
        assert_eq!(vic.peek(0x14), line as u8);
        assert_eq!(vic.peek(0x13), (cycle as u16 * 4) as u8);
    }

    #[test]
    fn light_pen_latches_once_per_frame() {
        let (mut vic, memory) = make_vic_and_memory();
        while vic.raster_line() < 50 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        let first_lpy = vic.peek(0x14);

        for _ in 0..200 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        assert_eq!(vic.peek(0x14), first_lpy);
    }

    #[test]
    fn unmapped_registers_return_last_bus_data() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..(CYCLES_PER_LINE as u32 * (DISPLAY_START_LINE as u32 + 2)) {
            tick_vic(&mut vic, &memory);
        }
        assert_eq!(vic.read(0x2F), vic.peek(0x2F));
        assert_eq!(vic.read(0x30), vic.read(0x2F));
        assert_eq!(vic.read(0x3F), vic.read(0x2F));
    }

    #[test]
    fn xscroll_zero_renders_cell_unchanged() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x08);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01;
        tick_vic(&mut vic, &memory);

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        for px in 0..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[1]);
        }
    }

    #[test]
    fn xscroll_four_shifts_cell_right() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x0C);
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01;
        tick_vic(&mut vic, &memory);

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        for px in 0..4 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[0]);
        }
        for px in 4..8 {
            assert_eq!(fb_pixel(&vic, fb_x0 + px, fb_y), PALETTE[1]);
        }
    }
}
