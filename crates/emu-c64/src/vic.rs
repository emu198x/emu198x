//! VIC-II 6569 (PAL) video chip.
//!
//! Implements text mode rendering, raster counter, raster IRQ, badline
//! cycle stealing, and single-colour sprites with priority.
//!
//! # Timing (PAL)
//!
//! - 312 raster lines per frame (0-311)
//! - 63 CPU cycles per line
//! - 19,656 CPU cycles per frame
//! - Display window: cycles 16-55, lines $30-$F7
//!
//! # Framebuffer
//!
//! 416 x 284 pixels (visible area including borders). Each tick renders
//! 8 pixels. The display window is centred within the border.

#![allow(clippy::cast_possible_truncation)]

use crate::memory::C64Memory;
use crate::palette::PALETTE;

/// Total raster lines per PAL frame.
const LINES_PER_FRAME: u16 = 312;

/// CPU cycles per raster line (PAL).
const CYCLES_PER_LINE: u8 = 63;

/// First visible raster line (top border start).
/// 42 lines of top border before display at $30 (48).
const FIRST_VISIBLE_LINE: u16 = 6;

/// Last visible raster line (bottom border end, exclusive).
/// 42 lines of bottom border after display ends at $F8 (248).
const LAST_VISIBLE_LINE: u16 = 290;

/// Visible lines in framebuffer.
const VISIBLE_LINES: u16 = LAST_VISIBLE_LINE - FIRST_VISIBLE_LINE;

/// First visible cycle in a line (left border start).
/// 6 cycles of left border before display at cycle 16.
const FIRST_VISIBLE_CYCLE: u8 = 10;

/// Last visible cycle (right border end, exclusive).
/// 6 cycles of right border after display ends at cycle 56.
const LAST_VISIBLE_CYCLE: u8 = 62;

/// Visible cycles per line.
const VISIBLE_CYCLES: u8 = LAST_VISIBLE_CYCLE - FIRST_VISIBLE_CYCLE;

/// Framebuffer width: visible cycles * 8 pixels per cycle.
pub const FB_WIDTH: u32 = VISIBLE_CYCLES as u32 * 8;

/// Framebuffer height: visible lines.
pub const FB_HEIGHT: u32 = VISIBLE_LINES as u32;

/// First line of the display window (where characters are rendered).
const DISPLAY_START_LINE: u16 = 0x30;

/// Last line of the display window (exclusive).
const DISPLAY_END_LINE: u16 = 0xF8;

/// First cycle of the display window (character data fetch area).
const DISPLAY_START_CYCLE: u8 = 16;

/// Last cycle of the display window (exclusive).
const DISPLAY_END_CYCLE: u8 = 56;

/// Offset to convert sprite X coordinate to framebuffer X coordinate.
///
/// Sprite X=24 corresponds to the left edge of the display window.
/// The display window starts at fb_x = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) * 8 = 48.
/// So fb_x = sprite_x - 24 + 48 = sprite_x + 24.
const SPRITE_X_TO_FB: i16 = 24;

/// VIC-II 6569 PAL chip.
pub struct Vic {
    /// VIC-II registers ($D000-$D02E).
    regs: [u8; 0x40],

    /// Current raster line (0-311).
    raster_line: u16,

    /// Current cycle within the line (0-62).
    raster_cycle: u8,

    /// Raster compare value for IRQ ($D012 + bit 7 of $D011).
    raster_compare: u16,

    /// IRQ status register ($D019).
    irq_status: u8,

    /// IRQ enable mask ($D01A).
    irq_enable: u8,

    /// Whether the current line is a badline (re-evaluated every cycle).
    is_badline: bool,

    /// DEN (Display ENable) latch — set when DEN=1 seen during line $30.
    den_latch: bool,

    /// Frame complete flag (set at end of frame, cleared by `take_frame_complete`).
    frame_complete: bool,

    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,

    /// Internal line buffer for the 40 screen codes on the current badline.
    screen_row: [u8; 40],

    /// Internal line buffer for the 40 colour values on the current badline.
    colour_row: [u8; 40],

    /// Current character row within a text line (0-7, derived from raster & 7).
    char_row: u8,

    /// VIC-II bank (0-3), set from CIA2 port A.
    vic_bank: u8,

    // --- Sprite state ---

    /// Per-sprite bitmap data for the current scanline (3 bytes each).
    sprite_data: [[u8; 3]; 8],

    /// Whether each sprite is active on the current scanline.
    sprite_active: [bool; 8],

    /// Sprite-sprite collision ($D01E). Clear-on-read.
    sprite_sprite_collision: u8,
    /// Sprite-background collision ($D01F). Clear-on-read.
    sprite_bg_collision: u8,
    /// Edge-detect: suppress re-triggering IRQ until register is read.
    sprite_sprite_irq_latched: bool,
    sprite_bg_irq_latched: bool,
    /// Text row index (0-24), set during fetch_screen_row for bitmap modes.
    text_row: u16,
}

impl Vic {
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: [0; 0x40],
            raster_line: 0,
            raster_cycle: 0,
            raster_compare: 0,
            irq_status: 0,
            irq_enable: 0,
            is_badline: false,
            den_latch: false,
            frame_complete: false,
            framebuffer: vec![0xFF00_0000; FB_WIDTH as usize * FB_HEIGHT as usize],
            screen_row: [0; 40],
            colour_row: [0; 40],
            char_row: 0,
            vic_bank: 0,
            sprite_data: [[0; 3]; 8],
            sprite_active: [false; 8],
            sprite_sprite_collision: 0,
            sprite_bg_collision: 0,
            sprite_sprite_irq_latched: false,
            sprite_bg_irq_latched: false,
            text_row: 0,
        }
    }

    /// Tick the VIC-II for one CPU cycle.
    ///
    /// Renders 8 pixels, advances the beam, detects badlines.
    /// Returns `true` if the CPU should be stalled this cycle (badline DMA).
    pub fn tick(&mut self, memory: &C64Memory) -> bool {
        // Fetch sprite data at the start of each visible line
        if self.raster_cycle == 0
            && self.raster_line >= FIRST_VISIBLE_LINE
            && self.raster_line < LAST_VISIBLE_LINE
        {
            self.fetch_sprite_data(memory);
        }

        // Render 8 pixels for this cycle
        self.render_pixels(memory);

        // Re-evaluate badline condition every cycle (DEN/YSCROLL can change mid-line).
        self.check_badline();

        // Stall CPU during badline DMA cycles 15–54 (40 cycles of character fetch).
        let cpu_stalled = self.is_badline && (15..=54).contains(&self.raster_cycle);

        // Fetch screen row data at the start of the badline DMA window.
        if self.is_badline && self.raster_cycle == 15 {
            self.char_row = 0;
            self.fetch_screen_row(memory);
        }

        // Advance beam position
        self.raster_cycle += 1;
        if self.raster_cycle >= CYCLES_PER_LINE {
            self.raster_cycle = 0;
            self.raster_line += 1;

            if self.raster_line >= LINES_PER_FRAME {
                self.raster_line = 0;
                self.frame_complete = true;
                self.den_latch = false;
            }

            // Increment the row counter (RC) at each line wrap within the display.
            // The real VIC-II increments RC at cycle 58. Badlines reset it to 0
            // (handled above at cycle 15). This gives a 0-7 count per text line
            // that's correct regardless of YSCROLL.
            if self.den_latch
                && (DISPLAY_START_LINE..DISPLAY_END_LINE).contains(&self.raster_line)
            {
                self.char_row = (self.char_row + 1) & 7;
            }
        }

        // Check raster compare IRQ
        if self.raster_line == self.raster_compare && self.raster_cycle == 0 {
            self.irq_status |= 0x01; // Set raster IRQ flag
        }

        cpu_stalled
    }

    /// Check if current cycle is within a badline.
    ///
    /// Real VIC-II re-evaluates the badline condition every cycle — DEN and
    /// YSCROLL writes take effect immediately.
    fn check_badline(&mut self) {
        let den = self.regs[0x11] & 0x10 != 0;
        let yscroll = u16::from(self.regs[0x11] & 0x07);

        // DEN latch: once DEN is seen as 1 during line $30, display stays enabled
        if self.raster_line == DISPLAY_START_LINE && den {
            self.den_latch = true;
        }

        self.is_badline = self.den_latch
            && (DISPLAY_START_LINE..DISPLAY_END_LINE).contains(&self.raster_line)
            && (self.raster_line & 7) == yscroll;
    }

    /// Fetch the 40 screen codes and colours for the current row.
    fn fetch_screen_row(&mut self, memory: &C64Memory) {
        let screen_base = self.screen_base();
        let text_row = ((self.raster_line - DISPLAY_START_LINE) / 8) as u16;
        self.text_row = text_row;

        for col in 0u16..40 {
            let screen_addr = screen_base + text_row * 40 + col;
            // VIC-II reads through its own bus (sees char ROM, not I/O)
            self.screen_row[col as usize] = memory.vic_read(self.vic_bank, screen_addr & 0x3FFF);
            self.colour_row[col as usize] = memory.colour_ram_read(text_row * 40 + col);
        }
    }

    /// Fetch sprite bitmap data for all active sprites on the current scanline.
    fn fetch_sprite_data(&mut self, memory: &C64Memory) {
        let sprite_enable = self.regs[0x15];
        let y_expand = self.regs[0x17];
        let screen_base = self.screen_base();

        for i in 0..8usize {
            self.sprite_active[i] = false;

            if sprite_enable & (1 << i) == 0 {
                continue;
            }

            let sprite_y = u16::from(self.regs[1 + i * 2]);
            let height = if y_expand & (1 << i) != 0 { 42u16 } else { 21u16 };

            // Check if this sprite is visible on the current raster line
            let line_in_sprite = self.raster_line.wrapping_sub(sprite_y);
            if line_in_sprite >= height {
                continue;
            }

            // Y-expand doubles each row
            let data_line = if y_expand & (1 << i) != 0 {
                line_in_sprite / 2
            } else {
                line_in_sprite
            } as u16;

            // Sprite pointer at screen_base + $3F8 + sprite_num
            let ptr_addr = screen_base + 0x03F8 + i as u16;
            let sprite_ptr = memory.vic_read(self.vic_bank, ptr_addr & 0x3FFF);

            // Sprite data at pointer * 64 + data_line * 3
            let data_base = u16::from(sprite_ptr) * 64 + data_line * 3;
            self.sprite_data[i][0] = memory.vic_read(self.vic_bank, data_base & 0x3FFF);
            self.sprite_data[i][1] = memory.vic_read(self.vic_bank, (data_base + 1) & 0x3FFF);
            self.sprite_data[i][2] = memory.vic_read(self.vic_bank, (data_base + 2) & 0x3FFF);
            self.sprite_active[i] = true;
        }
    }

    /// Render 8 pixels for the current beam position.
    fn render_pixels(&mut self, memory: &C64Memory) {
        // Check if we're in the visible area
        if self.raster_line < FIRST_VISIBLE_LINE || self.raster_line >= LAST_VISIBLE_LINE {
            return;
        }
        if self.raster_cycle < FIRST_VISIBLE_CYCLE || self.raster_cycle >= LAST_VISIBLE_CYCLE {
            return;
        }

        let fb_y = (self.raster_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x = (self.raster_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        let fb_offset = fb_y * FB_WIDTH as usize + fb_x;

        let border_colour = PALETTE[(self.regs[0x20] & 0x0F) as usize];
        let bg_colour = PALETTE[(self.regs[0x21] & 0x0F) as usize];

        // Are we in the display window?
        let in_display = self.den_latch
            && (DISPLAY_START_LINE..DISPLAY_END_LINE).contains(&self.raster_line)
            && (DISPLAY_START_CYCLE..DISPLAY_END_CYCLE).contains(&self.raster_cycle);

        // Track which of the 8 pixels are character foreground (for sprite priority)
        let mut fg_mask: u8 = 0;

        if !in_display {
            // Border area
            for px in 0..8usize {
                let idx = fb_offset + px;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = border_colour;
                }
            }
        } else {
            // Character display area
            let display_cycle = self.raster_cycle - DISPLAY_START_CYCLE;
            let col = display_cycle as usize;

            if col >= 40 {
                // Beyond 40 columns — draw border
                for px in 0..8usize {
                    let idx = fb_offset + px;
                    if idx < self.framebuffer.len() {
                        self.framebuffer[idx] = border_colour;
                    }
                }
            } else {
                let char_code = self.screen_row[col];
                let colour_nybble = self.colour_row[col];

                let bmm = self.regs[0x11] & 0x20 != 0;
                let ecm = self.regs[0x11] & 0x40 != 0;
                let mcm = self.regs[0x16] & 0x10 != 0;

                if ecm && (bmm || mcm) {
                    // Invalid mode combination: all pixels black
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = PALETTE[0];
                        }
                    }
                } else if bmm && mcm {
                    self.render_mcm_bitmap(
                        fb_offset, col, char_code, colour_nybble, &mut fg_mask, memory,
                    );
                } else if bmm {
                    self.render_hires_bitmap(
                        fb_offset, col, char_code, &mut fg_mask, memory,
                    );
                } else if ecm {
                    self.render_ecm_text(
                        fb_offset, col, char_code, colour_nybble, &mut fg_mask, memory,
                    );
                } else if mcm {
                    self.render_mcm_text(
                        fb_offset, col, char_code, colour_nybble, &mut fg_mask, memory,
                    );
                } else {
                    self.render_standard_text(
                        fb_offset, col, char_code, colour_nybble, &mut fg_mask, memory,
                    );
                }
            }
        }

        // Overlay sprites on top of the rendered pixels
        self.overlay_sprites(fb_offset, fb_x, fg_mask);

        // Trigger sprite collision IRQs
        if self.sprite_sprite_collision != 0 && !self.sprite_sprite_irq_latched {
            self.sprite_sprite_irq_latched = true;
            self.irq_status |= 0x04; // IMMC: sprite-sprite collision
        }
        if self.sprite_bg_collision != 0 && !self.sprite_bg_irq_latched {
            self.sprite_bg_irq_latched = true;
            self.irq_status |= 0x02; // IMBC: sprite-background collision
        }
    }

    /// Standard text mode: 320×200, 1 bit per pixel.
    fn render_standard_text(
        &mut self,
        fb_offset: usize,
        _col: usize,
        char_code: u8,
        colour_nybble: u8,
        fg_mask: &mut u8,
        memory: &C64Memory,
    ) {
        let bg_colour = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];

        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            let colour = if bit != 0 {
                *fg_mask |= 1 << px;
                fg_colour
            } else {
                bg_colour
            };
            let idx = fb_offset + px;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
            }
        }
    }

    /// Hires bitmap mode (BMM): 320×200, 1 bit per pixel.
    /// Colours from screen RAM: hi nybble = fg, lo nybble = bg.
    fn render_hires_bitmap(
        &mut self,
        fb_offset: usize,
        col: usize,
        char_code: u8,
        fg_mask: &mut u8,
        memory: &C64Memory,
    ) {
        let fg_colour = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let bg_colour = PALETTE[(char_code & 0x0F) as usize];

        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            let colour = if bit != 0 {
                *fg_mask |= 1 << px;
                fg_colour
            } else {
                bg_colour
            };
            let idx = fb_offset + px;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
            }
        }
    }

    /// Extended colour text mode (ECM): 320×200.
    /// Char code bits 6-7 select background from $D021-$D024.
    /// Only 64 characters available (bits 0-5).
    fn render_ecm_text(
        &mut self,
        fb_offset: usize,
        _col: usize,
        char_code: u8,
        colour_nybble: u8,
        fg_mask: &mut u8,
        memory: &C64Memory,
    ) {
        let bg_select = (char_code >> 6) & 0x03;
        let bg_colour = PALETTE[(self.regs[0x21 + bg_select as usize] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];

        let char_base = self.char_base();
        let effective_char = char_code & 0x3F;
        let bitmap_addr = char_base + u16::from(effective_char) * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            let colour = if bit != 0 {
                *fg_mask |= 1 << px;
                fg_colour
            } else {
                bg_colour
            };
            let idx = fb_offset + px;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
            }
        }
    }

    /// Multicolour text mode (MCM): per-character decision.
    /// Colour RAM bit 3 clear → standard text (8px wide).
    /// Colour RAM bit 3 set → bit-pair mode (4 double-wide pixels).
    fn render_mcm_text(
        &mut self,
        fb_offset: usize,
        _col: usize,
        char_code: u8,
        colour_nybble: u8,
        fg_mask: &mut u8,
        memory: &C64Memory,
    ) {
        if colour_nybble & 0x08 == 0 {
            // Bit 3 clear: standard text rendering for this character
            self.render_standard_text(fb_offset, _col, char_code, colour_nybble, fg_mask, memory);
            return;
        }

        // Bit 3 set: multicolour mode
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize]; // 00
        let bg1 = PALETTE[(self.regs[0x22] & 0x0F) as usize]; // 01
        let bg2 = PALETTE[(self.regs[0x23] & 0x0F) as usize]; // 10
        let fg_colour = PALETTE[(colour_nybble & 0x07) as usize]; // 11 (low 3 bits)

        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

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
                *fg_mask |= (1 << px0) | (1 << px1);
            }
            let idx0 = fb_offset + px0;
            let idx1 = fb_offset + px1;
            if idx0 < self.framebuffer.len() {
                self.framebuffer[idx0] = colour;
            }
            if idx1 < self.framebuffer.len() {
                self.framebuffer[idx1] = colour;
            }
        }
    }

    /// Multicolour bitmap mode (BMM+MCM): 160×200, bit pairs.
    /// 00=$D021, 01=screen hi, 10=screen lo, 11=colour RAM.
    fn render_mcm_bitmap(
        &mut self,
        fb_offset: usize,
        col: usize,
        char_code: u8,
        colour_nybble: u8,
        fg_mask: &mut u8,
        memory: &C64Memory,
    ) {
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize]; // 00
        let c01 = PALETTE[((char_code >> 4) & 0x0F) as usize]; // 01: screen hi
        let c10 = PALETTE[(char_code & 0x0F) as usize]; // 10: screen lo
        let c11 = PALETTE[(colour_nybble & 0x0F) as usize]; // 11: colour RAM

        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

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
                *fg_mask |= (1 << px0) | (1 << px1);
            }
            let idx0 = fb_offset + px0;
            let idx1 = fb_offset + px1;
            if idx0 < self.framebuffer.len() {
                self.framebuffer[idx0] = colour;
            }
            if idx1 < self.framebuffer.len() {
                self.framebuffer[idx1] = colour;
            }
        }
    }

    /// Overlay active sprites onto the 8 pixels just rendered.
    ///
    /// Handles both hires and multicolour sprites, collision detection,
    /// and sprite priority. Processes sprites in reverse priority order
    /// (7 = lowest first, 0 = highest last) for rendering.
    fn overlay_sprites(&mut self, fb_offset: usize, fb_x_start: usize, fg_mask: u8) {
        let priority = self.regs[0x1B];
        let x_expand = self.regs[0x1D];
        let mcm_reg = self.regs[0x1C];
        let mc0 = PALETTE[(self.regs[0x25] & 0x0F) as usize];
        let mc1 = PALETTE[(self.regs[0x26] & 0x0F) as usize];

        // Pass 1: build per-pixel coverage mask (which sprites have non-transparent
        // pixels at each of the 8 screen pixels) and colour for rendering.
        // sprite_coverage[px] = bitmask of sprites present at that pixel.
        // sprite_colour[px][i] = ARGB colour for sprite i at pixel px (if present).
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

                // X-expand doubles each pixel
                let data_pos = if expanded_x {
                    pixel_in_sprite / 2
                } else {
                    pixel_in_sprite
                } as usize;

                if is_mcm {
                    // Multicolour: bit pairs from the data, each pair = 2 data pixels
                    // In MCM, the 24 data bits become 12 bit-pairs.
                    // data_pos ranges 0-23; MCM pair index = data_pos / 2.
                    let pair_idx = data_pos / 2;
                    let byte_idx = pair_idx / 4;
                    let shift = 6 - (pair_idx % 4) * 2;
                    let bits = (self.sprite_data[i][byte_idx] >> shift) & 0x03;

                    if bits != 0b00 {
                        sprite_coverage[px] |= 1 << i;
                        sprite_colour[px][i] = match bits {
                            0b01 => mc0,
                            0b10 => sprite_col,
                            _ => mc1, // 0b11
                        };
                    }
                } else {
                    // Hires: single bits
                    let byte_idx = data_pos / 8;
                    let bit_idx = 7 - (data_pos % 8);

                    if self.sprite_data[i][byte_idx] & (1 << bit_idx) != 0 {
                        sprite_coverage[px] |= 1 << i;
                        sprite_colour[px][i] = sprite_col;
                    }
                }
            }
        }

        // Pass 2: collision detection (independent of priority/rendering)
        for px in 0..8usize {
            let cov = sprite_coverage[px];
            if cov.count_ones() >= 2 {
                self.sprite_sprite_collision |= cov;
            }
            if cov != 0 && (fg_mask & (1 << px)) != 0 {
                self.sprite_bg_collision |= cov;
            }
        }

        // Pass 3: render sprites in reverse priority order (7 first, 0 last)
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

    /// Screen memory base address within the VIC-II 16K bank.
    fn screen_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 4) & 0x0F) * 0x0400
    }

    /// Character memory base address within the VIC-II 16K bank.
    fn char_base(&self) -> u16 {
        u16::from((self.regs[0x18] >> 1) & 0x07) * 0x0800
    }

    /// Bitmap memory base address within the VIC-II 16K bank.
    fn bitmap_base(&self) -> u16 {
        if self.regs[0x18] & 0x08 != 0 {
            0x2000
        } else {
            0x0000
        }
    }

    /// Read a VIC-II register.
    ///
    /// `&mut self` because $D01E/$D01F are clear-on-read.
    pub fn read(&mut self, reg: u8) -> u8 {
        match reg & 0x3F {
            0x11 => {
                // $D011: Control reg 1 with current raster bit 8
                let raster_hi = if self.raster_line & 0x100 != 0 {
                    0x80
                } else {
                    0x00
                };
                (self.regs[0x11] & 0x7F) | raster_hi
            }
            0x12 => {
                // $D012: Raster counter low 8 bits
                (self.raster_line & 0xFF) as u8
            }
            0x19 => {
                // $D019: IRQ status — bit 7 is OR of all active & enabled flags
                let any_active = if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.irq_status | any_active
            }
            0x1A => self.irq_enable & 0x0F,
            0x1E => {
                // $D01E: Sprite-sprite collision — clear on read
                let val = self.sprite_sprite_collision;
                self.sprite_sprite_collision = 0;
                self.sprite_sprite_irq_latched = false;
                val
            }
            0x1F => {
                // $D01F: Sprite-background collision — clear on read
                let val = self.sprite_bg_collision;
                self.sprite_bg_collision = 0;
                self.sprite_bg_irq_latched = false;
                val
            }
            r if r <= 0x2E => self.regs[r as usize],
            // Unused registers return $FF
            _ => 0xFF,
        }
    }

    /// Read a VIC-II register without side effects (no clear-on-read).
    ///
    /// Used for observation/debugging when mutation is not desired.
    #[must_use]
    pub fn peek(&self, reg: u8) -> u8 {
        match reg & 0x3F {
            0x11 => {
                let raster_hi = if self.raster_line & 0x100 != 0 {
                    0x80
                } else {
                    0x00
                };
                (self.regs[0x11] & 0x7F) | raster_hi
            }
            0x12 => (self.raster_line & 0xFF) as u8,
            0x19 => {
                let any_active = if (self.irq_status & self.irq_enable & 0x0F) != 0 {
                    0x80
                } else {
                    0x00
                };
                self.irq_status | any_active
            }
            0x1A => self.irq_enable & 0x0F,
            0x1E => self.sprite_sprite_collision,
            0x1F => self.sprite_bg_collision,
            r if r <= 0x2E => self.regs[r as usize],
            _ => 0xFF,
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
                // Update raster compare bit 8
                self.raster_compare =
                    (self.raster_compare & 0x00FF) | (u16::from(value & 0x80) << 1);
            }
            0x12 => {
                // $D012 write: set raster compare low 8 bits
                self.raster_compare = (self.raster_compare & 0x0100) | u16::from(value);
            }
            0x19 => {
                // $D019 write: acknowledge IRQ by writing 1 bits
                self.irq_status &= !value & 0x0F;
            }
            0x1A => {
                self.irq_enable = value & 0x0F;
            }
            _ => {}
        }
    }

    /// Check if the VIC-II has an active IRQ.
    #[must_use]
    pub fn irq_active(&self) -> bool {
        (self.irq_status & self.irq_enable & 0x0F) != 0
    }

    /// Set the VIC-II bank (0-3) from CIA2 port A bits 0-1 (inverted).
    pub fn set_bank(&mut self, bank: u8) {
        self.vic_bank = bank & 0x03;
    }

    /// Get the current VIC-II bank.
    #[must_use]
    pub fn bank(&self) -> u8 {
        self.vic_bank
    }

    /// Reference to the framebuffer (ARGB32).
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
    pub const fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Check and clear the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let complete = self.frame_complete;
        self.frame_complete = false;
        complete
    }

    /// Current raster line.
    #[must_use]
    pub fn raster_line(&self) -> u16 {
        self.raster_line
    }

    /// Current cycle within the line.
    #[must_use]
    pub fn raster_cycle(&self) -> u8 {
        self.raster_cycle
    }

    /// Current character row (0-7, for debugging).
    #[must_use]
    pub fn char_row(&self) -> u8 {
        self.char_row
    }

    /// Whether the current line is a badline (for debugging).
    #[must_use]
    pub fn is_badline(&self) -> bool {
        self.is_badline
    }
}

impl Default for Vic {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vic_and_memory() -> (Vic, C64Memory) {
        let kernal = vec![0; 8192];
        let basic = vec![0; 8192];
        let chargen = vec![0xFF; 4096]; // All pixels set
        let vic = Vic::new();
        let memory = C64Memory::new(&kernal, &basic, &chargen);
        (vic, memory)
    }

    #[test]
    fn initial_state() {
        let mut vic = Vic::new();
        assert_eq!(vic.raster_line(), 0);
        assert_eq!(vic.raster_cycle(), 0);
        assert!(!vic.irq_active());
        assert!(!vic.take_frame_complete());
    }

    #[test]
    fn raster_advances() {
        let (mut vic, memory) = make_vic_and_memory();
        // Tick through one line (63 cycles)
        for _ in 0..63 {
            vic.tick(&memory);
        }
        assert_eq!(vic.raster_line(), 1);
        assert_eq!(vic.raster_cycle(), 0);
    }

    #[test]
    fn frame_complete_after_full_frame() {
        let (mut vic, memory) = make_vic_and_memory();
        let total_cycles = u32::from(LINES_PER_FRAME) * u32::from(CYCLES_PER_LINE);
        for _ in 0..total_cycles {
            vic.tick(&memory);
        }
        assert!(vic.take_frame_complete());
        assert!(!vic.take_frame_complete()); // Cleared after take
    }

    #[test]
    fn raster_irq() {
        let (mut vic, memory) = make_vic_and_memory();
        // Set raster compare to line 1
        vic.write(0x12, 1);
        vic.write(0x1A, 0x01); // Enable raster IRQ

        // Run through line 0 (63 cycles)
        for _ in 0..63 {
            vic.tick(&memory);
        }
        // At the start of line 1, raster IRQ should fire
        assert!(vic.irq_active());

        // Acknowledge the IRQ
        vic.write(0x19, 0x01);
        assert!(!vic.irq_active());
    }

    #[test]
    fn framebuffer_size() {
        let vic = Vic::new();
        assert_eq!(
            vic.framebuffer().len(),
            FB_WIDTH as usize * FB_HEIGHT as usize
        );
    }

    #[test]
    fn register_read_write() {
        let mut vic = Vic::new();
        vic.write(0x20, 0x06); // Border colour = blue
        assert_eq!(vic.read(0x20), 0x06);

        vic.write(0x21, 0x01); // Background = white
        assert_eq!(vic.read(0x21), 0x01);
    }

    #[test]
    fn bank_selection() {
        let mut vic = Vic::new();
        vic.set_bank(2);
        assert_eq!(vic.bank(), 2);
        vic.set_bank(5); // Should mask to 1
        assert_eq!(vic.bank(), 1);
    }

    #[test]
    fn sprite_renders_at_correct_position() {
        let (mut vic, mut memory) = make_vic_and_memory();

        // Set up sprite 0: enable, position, colour
        vic.write(0x15, 0x01); // Enable sprite 0
        vic.write(0x00, 172); // X = 172
        vic.write(0x01, 100); // Y = 100 (within display area)
        vic.write(0x27, 0x01); // Sprite 0 colour = white

        // Set screen base to default ($0400) → sprite pointers at $07F8
        vic.write(0x18, 0x14); // Screen at $0400, chars at $1000

        // Sprite pointer: $07F8 = screen_base($0400) + $3F8
        // Point to sprite data at address $80 * 64 = $2000
        memory.ram_write(0x07F8, 0x80);

        // Write a solid first row of sprite data at $2000
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        // Enable display (DEN) with YSCROLL=3 (standard)
        vic.write(0x11, 0x1B);

        // Run through frames until raster line 100, cycle past the sprite X
        let target_line = 100u16;
        let target_cycle = 35u8; // Within sprite X range

        // Advance to target line
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..cycles_to_target {
            vic.tick(&memory);
        }

        // Check framebuffer: sprite should have drawn white pixels
        // Sprite X=172, fb_x = 172 + 24 = 196
        // At raster line 100, fb_y = 100 - FIRST_VISIBLE_LINE = 94
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let sprite_fb_x = 196usize;
        let idx = fb_y * FB_WIDTH as usize + sprite_fb_x;

        let white = PALETTE[1]; // Colour index 1 = white
        assert_eq!(
            vic.framebuffer()[idx], white,
            "Sprite pixel at ({sprite_fb_x}, {fb_y}) should be white"
        );
    }

    #[test]
    fn bitmap_base_selection() {
        let mut vic = Vic::new();
        // Bit 3 clear → $0000
        vic.write(0x18, 0x14);
        assert_eq!(vic.bitmap_base(), 0x0000);
        // Bit 3 set → $2000
        vic.write(0x18, 0x1C);
        assert_eq!(vic.bitmap_base(), 0x2000);
    }

    #[test]
    fn collision_register_clear_on_read() {
        let mut vic = Vic::new();
        // Manually set collision fields
        vic.sprite_sprite_collision = 0x05;
        vic.sprite_bg_collision = 0x0A;

        // First read returns the collision value
        assert_eq!(vic.read(0x1E), 0x05);
        // Second read returns 0 (cleared)
        assert_eq!(vic.read(0x1E), 0x00);

        assert_eq!(vic.read(0x1F), 0x0A);
        assert_eq!(vic.read(0x1F), 0x00);
    }

    #[test]
    fn collision_peek_does_not_clear() {
        let mut vic = Vic::new();
        vic.sprite_sprite_collision = 0x03;
        // peek should not clear the register
        assert_eq!(vic.peek(0x1E), 0x03);
        assert_eq!(vic.peek(0x1E), 0x03);
        // read should clear it
        assert_eq!(vic.read(0x1E), 0x03);
        assert_eq!(vic.read(0x1E), 0x00);
    }

    #[test]
    fn invalid_mode_renders_black() {
        let (mut vic, memory) = make_vic_and_memory();

        // ECM + BMM = invalid mode
        vic.write(0x11, 0x7B); // DEN=1, YSCROLL=3, BMM=1, ECM=1
        vic.write(0x20, 0x06); // Border = blue
        vic.write(0x21, 0x01); // Background = white

        // Run to a display line, hitting a cell inside the display window
        let target_line = DISPLAY_START_LINE + 3; // First badline with YSCROLL=3
        let target_cycle = DISPLAY_START_CYCLE + 5; // Column 5
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            vic.tick(&memory);
        }

        // Check the pixel at the target position is black (PALETTE[0])
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x = (target_cycle - FIRST_VISIBLE_CYCLE) as usize * 8;
        let idx = fb_y * FB_WIDTH as usize + fb_x;
        assert_eq!(
            vic.framebuffer()[idx],
            PALETTE[0],
            "Invalid mode should render black"
        );
    }

    #[test]
    fn ecm_selects_background() {
        let kernal = vec![0; 8192];
        let basic = vec![0; 8192];
        let chargen = vec![0x00; 4096]; // All pixels clear → bg visible
        let memory = C64Memory::new(&kernal, &basic, &chargen);

        let mut vic = Vic::new();
        vic.write(0x11, 0x5B); // ECM + DEN + YSCROLL=3
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00); // BG0 = black
        vic.write(0x22, 0x02); // BG1 = red
        vic.write(0x23, 0x05); // BG2 = green
        vic.write(0x24, 0x06); // BG3 = blue

        // Advance to the first badline (line $33). Run past cycle 15 so
        // fetch_screen_row has already fired, then overwrite screen_row.
        let target_line = DISPLAY_START_LINE + 3;
        let past_fetch = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + 16;
        for _ in 0..past_fetch {
            vic.tick(&memory);
        }
        // Now at cycle 16 (DISPLAY_START_CYCLE). Overwrite screen_row after fetch.
        vic.screen_row[0] = 0x00; // BG0
        vic.screen_row[1] = 0x40; // BG1
        vic.screen_row[2] = 0x80; // BG2
        vic.screen_row[3] = 0xC0; // BG3

        // Tick column 0
        vic.tick(&memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        let idx0 = fb_y * FB_WIDTH as usize + fb_x0;
        assert_eq!(vic.framebuffer()[idx0], PALETTE[0], "ECM BG0 should be black");

        // Tick column 1
        vic.tick(&memory);
        let idx1 = fb_y * FB_WIDTH as usize + fb_x0 + 8;
        assert_eq!(vic.framebuffer()[idx1], PALETTE[2], "ECM BG1 should be red");

        // Tick column 2
        vic.tick(&memory);
        let idx2 = fb_y * FB_WIDTH as usize + fb_x0 + 16;
        assert_eq!(vic.framebuffer()[idx2], PALETTE[5], "ECM BG2 should be green");

        // Tick column 3
        vic.tick(&memory);
        let idx3 = fb_y * FB_WIDTH as usize + fb_x0 + 24;
        assert_eq!(vic.framebuffer()[idx3], PALETTE[6], "ECM BG3 should be blue");
    }

    #[test]
    fn mcm_text_bit3_selects_mode() {
        let kernal = vec![0; 8192];
        let basic = vec![0; 8192];
        // Chargen: char 0 = alternating bits for easy visual check
        let mut chargen = vec![0x00; 4096];
        chargen[0] = 0b10101010; // Char 0, row 0: bits 10 10 10 10

        let memory = C64Memory::new(&kernal, &basic, &chargen);
        let mut vic = Vic::new();

        // MCM mode
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x16, 0x18); // MCM=1 + XSCROLL=0
        vic.write(0x18, 0x14); // Screen $0400, chars $1000
        vic.write(0x21, 0x00); // BG0 = black (00)
        vic.write(0x22, 0x02); // BG1 = red (01)
        vic.write(0x23, 0x05); // BG2 = green (10)

        // Advance past the badline fetch at cycle 15
        let target_line = DISPLAY_START_LINE + 3;
        let past_fetch = u32::from(target_line) * u32::from(CYCLES_PER_LINE) + 16;
        for _ in 0..past_fetch {
            vic.tick(&memory);
        }

        // Now at cycle 16 (DISPLAY_START_CYCLE). Overwrite screen/colour rows.
        vic.screen_row[0] = 0; // Char 0
        vic.colour_row[0] = 0x0F; // Bit 3 set → MCM, low 3 bits = 7 (yellow for 11 pair)
        vic.screen_row[1] = 0;
        vic.colour_row[1] = 0x01; // Bit 3 clear → standard text, fg = white

        // Tick column 0 (MCM)
        vic.tick(&memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        let idx0 = fb_y * FB_WIDTH as usize + fb_x0;

        // Bitmap 0b10101010 → bit pairs: 10, 10, 10, 10
        // Pair 10 = BG2 colour = green (PALETTE[5])
        assert_eq!(
            vic.framebuffer()[idx0],
            PALETTE[5],
            "MCM pixel 0 should be green (pair 10)"
        );
        assert_eq!(
            vic.framebuffer()[idx0 + 1],
            PALETTE[5],
            "MCM pixel 1 should be green (pair 10, doubled)"
        );

        // Tick column 1 (standard text, bit 3 clear)
        vic.tick(&memory);
        let fb_x1 = fb_x0 + 8;
        let idx1 = fb_y * FB_WIDTH as usize + fb_x1;

        // Standard text: bitmap 0b10101010, fg = white (PALETTE[1]), bg = black (PALETTE[0])
        assert_eq!(
            vic.framebuffer()[idx1],
            PALETTE[1],
            "Standard text pixel 0 should be white (fg)"
        );
        assert_eq!(
            vic.framebuffer()[idx1 + 1],
            PALETTE[0],
            "Standard text pixel 1 should be black (bg)"
        );
    }

    #[test]
    fn sprite_mcm_bit_pairs() {
        let (mut vic, mut memory) = make_vic_and_memory();

        // Enable sprite 0 in MCM mode
        vic.write(0x15, 0x01); // Enable sprite 0
        vic.write(0x1C, 0x01); // Sprite 0 = multicolour
        vic.write(0x00, 172); // X = 172
        vic.write(0x01, 100); // Y = 100
        vic.write(0x25, 0x02); // MC0 = red
        vic.write(0x27, 0x05); // Sprite 0 colour = green (pair 10)
        vic.write(0x26, 0x06); // MC1 = blue
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3

        // Sprite pointer at $07F8
        memory.ram_write(0x07F8, 0x80);

        // Sprite data at $2000: first byte = 0b01_10_11_00
        // Pairs: 01=MC0(red), 10=sprite(green), 11=MC1(blue), 00=transparent
        memory.ram_write(0x2000, 0b01_10_11_00);
        memory.ram_write(0x2001, 0x00);
        memory.ram_write(0x2002, 0x00);

        // Sprite fb_x = 172 + 24 = 196. MCM pairs each cover 2 screen pixels:
        //   pair 0 (01) → pixels 196-197
        //   pair 1 (10) → pixels 198-199
        //   pair 2 (11) → pixels 200-201
        //   pair 3 (00) → pixels 202-203 (transparent)
        // Cycle 34 renders fb_x 192-199, cycle 35 renders fb_x 200-207.
        // Need both cycles to have rendered.
        let target_line = 100u16;
        let target_cycle = 35u8; // Render through cycle 35
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            vic.tick(&memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let base_idx = fb_y * FB_WIDTH as usize;

        // MCM pair 0 (bits 01) = MC0 = red, covers pixels 196-197
        assert_eq!(
            vic.framebuffer()[base_idx + 196],
            PALETTE[2],
            "MCM pair 01 pixel 0 should be red (MC0)"
        );
        assert_eq!(
            vic.framebuffer()[base_idx + 197],
            PALETTE[2],
            "MCM pair 01 pixel 1 should be red (MC0)"
        );

        // MCM pair 1 (bits 10) = sprite colour = green, covers pixels 198-199
        assert_eq!(
            vic.framebuffer()[base_idx + 198],
            PALETTE[5],
            "MCM pair 10 pixel 0 should be green (sprite colour)"
        );

        // MCM pair 2 (bits 11) = MC1 = blue, covers pixels 200-201
        assert_eq!(
            vic.framebuffer()[base_idx + 200],
            PALETTE[6],
            "MCM pair 11 pixel 0 should be blue (MC1)"
        );
    }

    #[test]
    fn sprite_sprite_collision() {
        let (mut vic, mut memory) = make_vic_and_memory();

        // Enable sprites 0 and 1 at the same position
        vic.write(0x15, 0x03); // Enable sprites 0 and 1
        vic.write(0x00, 172); // Sprite 0 X
        vic.write(0x01, 100); // Sprite 0 Y
        vic.write(0x02, 172); // Sprite 1 X (same as sprite 0)
        vic.write(0x03, 100); // Sprite 1 Y (same as sprite 0)
        vic.write(0x27, 0x01); // Sprite 0 colour = white
        vic.write(0x28, 0x02); // Sprite 1 colour = red
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);

        // Both sprites point to the same data
        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        // Run to where sprites overlap
        let target_line = 100u16;
        let target_cycle = 35u8;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            vic.tick(&memory);
        }

        // $D01E should have bits 0 and 1 set (sprites 0 and 1 collided)
        let collision = vic.read(0x1E);
        assert_eq!(
            collision & 0x03,
            0x03,
            "Sprites 0 and 1 should collide, got {collision:#04X}"
        );

        // After reading, register should be cleared
        assert_eq!(vic.read(0x1E), 0x00, "$D01E should be cleared after read");
    }

    #[test]
    fn sprite_bg_collision() {
        let (mut vic, mut memory) = make_vic_and_memory();

        // chargen is 0xFF (all fg pixels) — sprite overlapping fg triggers collision
        vic.write(0x15, 0x01); // Enable sprite 0
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x18, 0x14); // Screen $0400, chars $1000

        // Place sprite 0 within display window
        // Display window fb starts at cycle 16, fb_x = (16-10)*8 = 48
        // Sprite X that maps to display: fb_x = sprite_x + 24
        // For sprite to overlap char column 0: fb_x = 48, sprite_x = 24
        vic.write(0x00, 24); // X = 24 → fb_x = 48 (display window start)
        vic.write(0x01, 51); // Y = 51 (first badline with YSCROLL=3)
        vic.write(0x27, 0x01); // Sprite 0 colour = white

        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        // Set colour RAM to non-zero so char fg is rendered
        memory.colour_ram_write(0, 0x01);

        // Run to where sprite overlaps character fg
        let target_line = 51u16;
        let target_cycle = DISPLAY_START_CYCLE + 1;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            vic.tick(&memory);
        }

        // $D01F should have bit 0 set (sprite 0 collided with background)
        let collision = vic.read(0x1F);
        assert_ne!(
            collision & 0x01,
            0x00,
            "Sprite 0 should collide with bg, got {collision:#04X}"
        );
    }
}
