//! VIC-II video chip (6569 PAL / 6567 NTSC).
//!
//! Implements text mode rendering, raster counter, raster IRQ, badline
//! cycle stealing, sprite DMA stealing, and single-colour sprites with
//! priority.
//!
//! # Timing
//!
//! **PAL (6569):** 312 lines, 63 cycles/line, 19,656 cycles/frame.
//! **NTSC (6567):** 263 lines, 65 cycles/line, 17,095 cycles/frame.
//!
//! # Framebuffer
//!
//! Visible area including borders. Each tick renders 8 pixels.

#![allow(clippy::cast_possible_truncation)]

pub mod palette;

use palette::PALETTE;

// --- PAL defaults (used for the public constants) ---

/// PAL first visible line.
const PAL_FIRST_VISIBLE_LINE: u16 = 6;
/// PAL last visible line (exclusive).
const PAL_LAST_VISIBLE_LINE: u16 = 290;

/// NTSC first visible line.
const NTSC_FIRST_VISIBLE_LINE: u16 = 14;
/// NTSC last visible line (exclusive).
const NTSC_LAST_VISIBLE_LINE: u16 = 258;

/// First visible cycle in a line (left border start).
const FIRST_VISIBLE_CYCLE: u8 = 10;

/// Last visible cycle (right border end, exclusive).
/// Both PAL (63 cycles) and NTSC (65 cycles) have the same visible
/// horizontal range — the extra 2 NTSC cycles are in the HBLANK.
const LAST_VISIBLE_CYCLE: u8 = 62;

/// Visible cycles per line.
const VISIBLE_CYCLES: u8 = LAST_VISIBLE_CYCLE - FIRST_VISIBLE_CYCLE;

/// Default framebuffer width (PAL): visible cycles * 8 pixels.
pub const FB_WIDTH: u32 = VISIBLE_CYCLES as u32 * 8;

/// Default framebuffer height (PAL): visible lines.
pub const FB_HEIGHT: u32 = (PAL_LAST_VISIBLE_LINE - PAL_FIRST_VISIBLE_LINE) as u32;

/// First line of the display window (where characters are rendered).
const DISPLAY_START_LINE: u16 = 0x30;

/// Last line where the VIC-II can start a new badline (exclusive).
///
/// Badlines occur in the $30-$F7 range. Outside this range the data
/// sequencer continues outputting from the current text row but no new
/// screen-row fetches are triggered.
const DISPLAY_END_LINE: u16 = 0xF8;

/// First cycle of the display window (character data fetch area).
const DISPLAY_START_CYCLE: u8 = 16;

/// Last cycle of the display window (exclusive).
const DISPLAY_END_CYCLE: u8 = 56;

/// Offset to convert sprite X coordinate to framebuffer X coordinate.
///
/// Sprite X=24 corresponds to the left edge of the display window.
/// The display window starts at `fb_x` = (`DISPLAY_START_CYCLE` - `FIRST_VISIBLE_CYCLE`) * 8 = 48.
/// So `fb_x` = `sprite_x` - 24 + 48 = `sprite_x` + 24.
const SPRITE_X_TO_FB: i16 = 24;

/// VIC-II model variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VicModel {
    /// PAL 6569: 312 lines, 63 cycles/line.
    Pal6569,
    /// NTSC 6567: 263 lines, 65 cycles/line.
    Ntsc6567,
}

impl VicModel {
    /// Total raster lines per frame.
    #[must_use]
    pub fn lines_per_frame(self) -> u16 {
        match self {
            Self::Pal6569 => 312,
            Self::Ntsc6567 => 263,
        }
    }

    /// CPU cycles per raster line.
    #[must_use]
    pub fn cycles_per_line(self) -> u8 {
        match self {
            Self::Pal6569 => 63,
            Self::Ntsc6567 => 65,
        }
    }
}

/// 8 pixels of rendered cell data, returned by each render method.
struct CellPixels {
    /// ARGB colour for each of the 8 pixels.
    colour: [u32; 8],
    /// Bitmask: bit N set if pixel N is foreground (for sprite priority).
    fg_mask: u8,
}

impl CellPixels {
    /// All 8 pixels the same colour, no foreground.
    fn solid(c: u32) -> Self {
        Self {
            colour: [c; 8],
            fg_mask: 0,
        }
    }
}

/// VIC-II chip (6569 PAL / 6567 NTSC).
pub struct Vic {
    /// VIC-II registers ($D000-$D02E).
    regs: [u8; 0x40],

    /// Current raster line.
    raster_line: u16,

    /// Current cycle within the line.
    raster_cycle: u8,

    /// Raster compare value for IRQ ($D012 + bit 7 of $D011).
    raster_compare: u16,

    /// IRQ status register ($D019).
    irq_status: u8,

    /// IRQ enable mask ($D01A).
    irq_enable: u8,

    /// Whether the current line is a badline (re-evaluated every cycle).
    is_badline: bool,

    /// DEN (Display `ENable`) latch — set when DEN=1 seen during line $30.
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

    /// Whether each sprite has DMA active on the current line.
    sprite_dma_active: [bool; 8],

    /// Sprite-sprite collision ($D01E). Clear-on-read.
    sprite_sprite_collision: u8,
    /// Sprite-background collision ($D01F). Clear-on-read.
    sprite_bg_collision: u8,
    /// Edge-detect: suppress re-triggering IRQ until register is read.
    sprite_sprite_irq_latched: bool,
    sprite_bg_irq_latched: bool,
    /// Text row index (0-24), set during `fetch_screen_row` for bitmap modes.
    text_row: u16,

    /// XSCROLL carry: 8 pixels of ARGB colour from the previous column.
    xscroll_carry_pixels: [u32; 8],
    /// XSCROLL carry: `fg_mask` bits from the previous column.
    xscroll_carry_fg: u8,
    /// XSCROLL value latched at the start of each display line.
    xscroll_latch: u8,

    // --- Model-dependent timing ---
    /// Total raster lines per frame (312 PAL / 263 NTSC).
    lines_per_frame: u16,
    /// CPU cycles per raster line (63 PAL / 65 NTSC).
    cycles_per_line: u8,
    /// First visible raster line.
    first_visible_line: u16,
    /// Last visible raster line (exclusive).
    last_visible_line: u16,

    /// Light pen triggered this frame (cleared at frame start).
    lp_triggered: bool,
    /// Last byte fetched by VIC from memory (for floating bus reads).
    last_bus_data: u8,
}

impl Vic {
    /// Create a new VIC-II for the given model.
    #[must_use]
    pub fn new(model: VicModel) -> Self {
        let (first_vis, last_vis) = match model {
            VicModel::Pal6569 => (PAL_FIRST_VISIBLE_LINE, PAL_LAST_VISIBLE_LINE),
            VicModel::Ntsc6567 => (NTSC_FIRST_VISIBLE_LINE, NTSC_LAST_VISIBLE_LINE),
        };
        let visible_lines = u32::from(last_vis - first_vis);
        let fb_size = FB_WIDTH as usize * visible_lines as usize;

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

    /// Tick the VIC-II for one CPU cycle.
    ///
    /// Renders 8 pixels, advances the beam, detects badlines.
    /// Returns `true` if the CPU should be stalled this cycle (badline DMA).
    ///
    /// `read_vram` reads a byte from the VIC's video memory space. The address
    /// is a full 16-bit address with the bank already folded in. The system
    /// layer handles character ROM visibility at $1000-$1FFF in banks 0/2.
    ///
    /// `read_colour` reads a byte from colour RAM at the given offset (0-1023).
    pub fn tick(&mut self, read_vram: &dyn Fn(u16) -> u8, read_colour: &dyn Fn(u16) -> u8) -> bool {
        // Evaluate sprite DMA at cycle 55 (which sprites are active for
        // DMA on this line, based on Y-coordinate comparison).
        if self.raster_cycle == 55 {
            self.evaluate_sprite_dma();
        }

        // Fetch sprite data at the start of each visible line
        if self.raster_cycle == 0
            && self.raster_line >= self.first_visible_line
            && self.raster_line < self.last_visible_line
        {
            self.fetch_sprite_data(read_vram);
        }

        // Render 8 pixels for this cycle
        self.render_pixels(read_vram);

        // Re-evaluate badline condition every cycle (DEN/YSCROLL can change mid-line).
        self.check_badline();

        // Stall CPU during badline DMA cycles 15–54 or sprite DMA slots.
        let badline_stall = self.is_badline && (15..=54).contains(&self.raster_cycle);
        let sprite_stall = self.is_sprite_dma_stealing();
        let cpu_stalled = badline_stall || sprite_stall;

        // Fetch screen row data at the start of the badline DMA window.
        if self.is_badline && self.raster_cycle == 15 {
            self.char_row = 0;
            self.fetch_screen_row(read_vram, read_colour);
        }

        // Advance beam position
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

            // Increment the row counter (RC) at each line wrap within the display.
            // The real VIC-II increments RC at cycle 58. Badlines reset it to 0
            // (handled above at cycle 15). This gives a 0-7 count per text line
            // that's correct regardless of YSCROLL.
            // The range extends to $FB (RSEL=1 border close) so the last text
            // row's char_rows 5-7 on lines $F8-$FA render correctly.
            if self.den_latch && (DISPLAY_START_LINE..0xFBu16).contains(&self.raster_line) {
                self.char_row = (self.char_row + 1) & 7;
            }
        }

        // Check raster compare IRQ
        if self.raster_line == self.raster_compare && self.raster_cycle == 0 {
            self.irq_status |= 0x01; // Set raster IRQ flag
        }

        cpu_stalled
    }

    /// Compute the full 16-bit VRAM address from a bank-relative offset.
    fn vram_addr(&self, bank_offset: u16) -> u16 {
        u16::from(self.vic_bank) * 0x4000 + (bank_offset & 0x3FFF)
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
    fn fetch_screen_row(&mut self, read_vram: &dyn Fn(u16) -> u8, read_colour: &dyn Fn(u16) -> u8) {
        let screen_base = self.screen_base();
        let text_row = (self.raster_line - DISPLAY_START_LINE) / 8;
        self.text_row = text_row;

        for col in 0u16..40 {
            let screen_addr = screen_base + text_row * 40 + col;
            let byte = read_vram(self.vram_addr(screen_addr));
            self.screen_row[col as usize] = byte;
            self.last_bus_data = byte;
            self.colour_row[col as usize] = read_colour(text_row * 40 + col);
        }
    }

    /// Fetch sprite bitmap data for all active sprites on the current scanline.
    fn fetch_sprite_data(&mut self, read_vram: &dyn Fn(u16) -> u8) {
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
            };

            // Sprite pointer at screen_base + $3F8 + sprite_num
            let ptr_addr = screen_base + 0x03F8 + i as u16;
            let sprite_ptr = read_vram(self.vram_addr(ptr_addr));
            self.last_bus_data = sprite_ptr;

            // Sprite data at pointer * 64 + data_line * 3
            let data_base = u16::from(sprite_ptr) * 64 + data_line * 3;
            self.sprite_data[i][0] = read_vram(self.vram_addr(data_base));
            self.sprite_data[i][1] = read_vram(self.vram_addr(data_base + 1));
            self.sprite_data[i][2] = read_vram(self.vram_addr(data_base + 2));
            self.last_bus_data = self.sprite_data[i][2];
            self.sprite_active[i] = true;
        }
    }

    /// Render 8 pixels for the current beam position.
    fn render_pixels(&mut self, read_vram: &dyn Fn(u16) -> u8) {
        // Check if we're in the visible area
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

        // Are we in the character data area? The data sequencer runs whenever
        // the vertical border flip-flop is off. With RSEL=1 that's $33-$FA,
        // with RSEL=0 it's $37-$F6. Use the same vstart/vstop as the border.
        let rsel = self.regs[0x11] & 0x08 != 0;
        let char_vstart = if rsel { 0x33u16 } else { 0x37u16 };
        let char_vstop = if rsel { 0xFBu16 } else { 0xF7u16 };
        let in_char_area = self.den_latch
            && (char_vstart..char_vstop).contains(&self.raster_line)
            && (DISPLAY_START_CYCLE..DISPLAY_END_CYCLE).contains(&self.raster_cycle);

        // Track which of the 8 pixels are character foreground (for sprite priority)
        let mut fg_mask: u8 = 0;

        // At the first display cycle, latch XSCROLL and initialise carry to bg colour
        if self.raster_cycle == DISPLAY_START_CYCLE && in_char_area {
            self.xscroll_latch = self.regs[0x16] & 0x07;
            let bg = PALETTE[(self.regs[0x21] & 0x0F) as usize];
            self.xscroll_carry_pixels = [bg; 8];
            self.xscroll_carry_fg = 0;
        }

        if in_char_area {
            // Character display area — render cell data (even if not in visible
            // window, to keep the XSCROLL carry pipeline correct).
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
                    self.render_mcm_bitmap(col, char_code, colour_nybble, read_vram)
                } else if bmm {
                    self.render_hires_bitmap(col, char_code, read_vram)
                } else if ecm {
                    self.render_ecm_text(char_code, colour_nybble, read_vram)
                } else if mcm {
                    self.render_mcm_text(char_code, colour_nybble, read_vram)
                } else {
                    self.render_standard_text(char_code, colour_nybble, read_vram)
                };

                let xscroll = self.xscroll_latch as usize;

                if xscroll == 0 {
                    // Fast path: no scroll, write cell directly
                    for px in 0..8usize {
                        let idx = fb_offset + px;
                        if idx < self.framebuffer.len() {
                            self.framebuffer[idx] = cell.colour[px];
                        }
                    }
                    fg_mask = cell.fg_mask;
                } else {
                    // Composite: carry fills pixels 0..xscroll-1,
                    // cell fills pixels xscroll..7
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
                    // Save carry: rightmost xscroll pixels from cell
                    for i in 0..xscroll {
                        self.xscroll_carry_pixels[i] = cell.colour[8 - xscroll + i];
                    }
                    self.xscroll_carry_fg =
                        (cell.fg_mask >> (8 - xscroll)) & ((1u8 << xscroll) - 1);
                }
            }
        }

        // Visible window: CSEL/RSEL control display borders.
        // RSEL=1: rows $33-$FA, RSEL=0: rows $37-$F6 (24-row mode)
        // CSEL=1: cycles 16-55, CSEL=0: cycles 17-54 (38-column mode)
        let rsel = self.regs[0x11] & 0x08 != 0;
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
            // Overwrite with border colour (covers CSEL=0 edges, RSEL=0
            // top/bottom, and the normal outer border).
            for px in 0..8usize {
                let idx = fb_offset + px;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = border_colour;
                }
            }
            fg_mask = 0;
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
        &self,
        char_code: u8,
        colour_nybble: u8,
        read_vram: &dyn Fn(u16) -> u8,
    ) -> CellPixels {
        let bg_colour = PALETTE[(self.regs[0x21] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];

        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = read_vram(self.vram_addr(bitmap_addr));

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

    /// Hires bitmap mode (BMM): 320×200, 1 bit per pixel.
    /// Colours from screen RAM: hi nybble = fg, lo nybble = bg.
    fn render_hires_bitmap(
        &self,
        col: usize,
        char_code: u8,
        read_vram: &dyn Fn(u16) -> u8,
    ) -> CellPixels {
        let fg_colour = PALETTE[((char_code >> 4) & 0x0F) as usize];
        let bg_colour = PALETTE[(char_code & 0x0F) as usize];

        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = read_vram(self.vram_addr(bitmap_addr));

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

    /// Extended colour text mode (ECM): 320×200.
    /// Char code bits 6-7 select background from $D021-$D024.
    /// Only 64 characters available (bits 0-5).
    fn render_ecm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        read_vram: &dyn Fn(u16) -> u8,
    ) -> CellPixels {
        let bg_select = (char_code >> 6) & 0x03;
        let bg_colour = PALETTE[(self.regs[0x21 + bg_select as usize] & 0x0F) as usize];
        let fg_colour = PALETTE[(colour_nybble & 0x0F) as usize];

        let char_base = self.char_base();
        let effective_char = char_code & 0x3F;
        let bitmap_addr = char_base + u16::from(effective_char) * 8 + u16::from(self.char_row);
        let bitmap = read_vram(self.vram_addr(bitmap_addr));

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

    /// Multicolour text mode (MCM): per-character decision.
    /// Colour RAM bit 3 clear → standard text (8px wide).
    /// Colour RAM bit 3 set → bit-pair mode (4 double-wide pixels).
    fn render_mcm_text(
        &self,
        char_code: u8,
        colour_nybble: u8,
        read_vram: &dyn Fn(u16) -> u8,
    ) -> CellPixels {
        if colour_nybble & 0x08 == 0 {
            // Bit 3 clear: standard text rendering for this character
            return self.render_standard_text(char_code, colour_nybble, read_vram);
        }

        // Bit 3 set: multicolour mode
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize]; // 00
        let bg1 = PALETTE[(self.regs[0x22] & 0x0F) as usize]; // 01
        let bg2 = PALETTE[(self.regs[0x23] & 0x0F) as usize]; // 10
        let fg_colour = PALETTE[(colour_nybble & 0x07) as usize]; // 11 (low 3 bits)

        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = read_vram(self.vram_addr(bitmap_addr));

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

    /// Multicolour bitmap mode (BMM+MCM): 160×200, bit pairs.
    /// 00=$D021, 01=screen hi, 10=screen lo, 11=colour RAM.
    fn render_mcm_bitmap(
        &self,
        col: usize,
        char_code: u8,
        colour_nybble: u8,
        read_vram: &dyn Fn(u16) -> u8,
    ) -> CellPixels {
        let bg0 = PALETTE[(self.regs[0x21] & 0x0F) as usize]; // 00
        let c01 = PALETTE[((char_code >> 4) & 0x0F) as usize]; // 01: screen hi
        let c10 = PALETTE[(char_code & 0x0F) as usize]; // 10: screen lo
        let c11 = PALETTE[(colour_nybble & 0x0F) as usize]; // 11: colour RAM

        let bitmap_base = self.bitmap_base();
        let bitmap_addr =
            bitmap_base + self.text_row * 40 * 8 + col as u16 * 8 + u16::from(self.char_row);
        let bitmap = read_vram(self.vram_addr(bitmap_addr));

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
        for (px, &cov) in sprite_coverage.iter().enumerate() {
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

    /// Evaluate which sprites need DMA on this line.
    ///
    /// Called at cycle 55 of each line. Compares each enabled sprite's Y
    /// coordinate against the current raster line.
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

    /// Check whether the current cycle falls in a sprite DMA fetch slot.
    ///
    /// PAL VIC-II (6569) sprite DMA timing:
    ///   Sprite 0: cycles 58, 59
    ///   Sprite 1: cycles 60, 61
    ///   Sprite 2: cycles 62, 0 (wraps)
    ///   Sprite 3: cycles 1, 2
    ///   Sprite 4: cycles 3, 4
    ///   Sprite 5: cycles 5, 6
    ///   Sprite 6: cycles 7, 8
    ///   Sprite 7: cycles 9, 10
    ///
    /// NTSC (6567) has the same slot layout, just shifted by the extra cycles.
    /// Each active sprite steals 2 CPU cycles.
    fn is_sprite_dma_stealing(&self) -> bool {
        let c = self.raster_cycle;
        // Check each sprite's 2-cycle DMA window
        (self.sprite_dma_active[0] && (c == 58 || c == 59))
            || (self.sprite_dma_active[1] && (c == 60 || c == 61))
            || (self.sprite_dma_active[2] && (c == 62 || c == 0))
            || (self.sprite_dma_active[3] && (c == 1 || c == 2))
            || (self.sprite_dma_active[4] && (c == 3 || c == 4))
            || (self.sprite_dma_active[5] && (c == 5 || c == 6))
            || (self.sprite_dma_active[6] && (c == 7 || c == 8))
            || (self.sprite_dma_active[7] && (c == 9 || c == 10))
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
            // Unmapped registers ($2F-$3F) return the last byte VIC fetched
            // from memory (floating bus behaviour).
            _ => self.last_bus_data,
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

    /// Trigger light pen latch on LP pin falling edge.
    ///
    /// Latches the beam position into $D013 (LPX) and $D014 (LPY). Once
    /// latched, the values stay until the start of the next frame.
    pub fn trigger_light_pen(&mut self) {
        if self.lp_triggered {
            return;
        }
        self.lp_triggered = true;
        // LPX: raster cycle converted to pixel-pair units (divide X pixel by 2).
        // Each cycle = 8 pixels, so pixel-pair = cycle * 4.
        self.regs[0x13] = (u16::from(self.raster_cycle) * 4) as u8;
        self.regs[0x14] = self.raster_line as u8;
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
        Self::new(VicModel::Pal6569)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PAL constants for tests
    const LINES_PER_FRAME: u16 = 312;
    const CYCLES_PER_LINE: u8 = 63;
    const FIRST_VISIBLE_LINE: u16 = PAL_FIRST_VISIBLE_LINE;

    /// Test memory: 64K RAM + 4K character ROM at $1000-$1FFF in banks 0/2.
    struct TestMemory {
        ram: Box<[u8; 0x10000]>,
        char_rom: Vec<u8>,
    }

    impl TestMemory {
        fn new(chargen: &[u8]) -> Self {
            Self {
                ram: Box::new([0; 0x10000]),
                char_rom: chargen.to_vec(),
            }
        }

        fn read_vram(&self, addr: u16) -> u8 {
            let bank = (addr >> 14) & 0x03;
            let bank_addr = addr & 0x3FFF;
            if (bank == 0 || bank == 2) && (0x1000..0x2000).contains(&bank_addr) {
                self.char_rom[(bank_addr - 0x1000) as usize]
            } else {
                self.ram[addr as usize]
            }
        }

        fn read_colour(&self, _offset: u16) -> u8 {
            0
        }

        fn ram_write(&mut self, addr: u16, value: u8) {
            self.ram[addr as usize] = value;
        }
    }

    fn make_vic_and_memory() -> (Vic, TestMemory) {
        let chargen = vec![0xFF; 4096]; // All pixels set
        let vic = Vic::new(VicModel::Pal6569);
        let memory = TestMemory::new(&chargen);
        (vic, memory)
    }

    /// Tick the VIC with test memory closures.
    fn tick_vic(vic: &mut Vic, mem: &TestMemory) -> bool {
        vic.tick(&|addr| mem.read_vram(addr), &|off| mem.read_colour(off))
    }

    /// Tick the VIC with custom colour RAM closure.
    fn tick_vic_with_colour(vic: &mut Vic, mem: &TestMemory, colour_ram: &[u8]) -> bool {
        vic.tick(&|addr| mem.read_vram(addr), &|off| {
            if (off as usize) < colour_ram.len() {
                colour_ram[off as usize] & 0x0F
            } else {
                0
            }
        })
    }

    #[test]
    fn initial_state() {
        let mut vic = Vic::new(VicModel::Pal6569);
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
            tick_vic(&mut vic, &memory);
        }
        // At the start of line 1, raster IRQ should fire
        assert!(vic.irq_active());

        // Acknowledge the IRQ
        vic.write(0x19, 0x01);
        assert!(!vic.irq_active());
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
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x20, 0x06); // Border colour = blue
        assert_eq!(vic.read(0x20), 0x06);

        vic.write(0x21, 0x01); // Background = white
        assert_eq!(vic.read(0x21), 0x01);
    }

    #[test]
    fn bank_selection() {
        let mut vic = Vic::new(VicModel::Pal6569);
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
            tick_vic(&mut vic, &memory);
        }

        // Check framebuffer: sprite should have drawn white pixels
        // Sprite X=172, fb_x = 172 + 24 = 196
        // At raster line 100, fb_y = 100 - FIRST_VISIBLE_LINE = 94
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let sprite_fb_x = 196usize;
        let idx = fb_y * FB_WIDTH as usize + sprite_fb_x;

        let white = PALETTE[1]; // Colour index 1 = white
        assert_eq!(
            vic.framebuffer()[idx],
            white,
            "Sprite pixel at ({sprite_fb_x}, {fb_y}) should be white"
        );
    }

    #[test]
    fn bitmap_base_selection() {
        let mut vic = Vic::new(VicModel::Pal6569);
        // Bit 3 clear → $0000
        vic.write(0x18, 0x14);
        assert_eq!(vic.bitmap_base(), 0x0000);
        // Bit 3 set → $2000
        vic.write(0x18, 0x1C);
        assert_eq!(vic.bitmap_base(), 0x2000);
    }

    #[test]
    fn collision_register_clear_on_read() {
        let mut vic = Vic::new(VicModel::Pal6569);
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
        let mut vic = Vic::new(VicModel::Pal6569);
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
            tick_vic(&mut vic, &memory);
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
        let chargen = vec![0x00; 4096]; // All pixels clear → bg visible
        let memory = TestMemory::new(&chargen);

        let mut vic = Vic::new(VicModel::Pal6569);
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
            tick_vic(&mut vic, &memory);
        }
        // Now at cycle 16 (DISPLAY_START_CYCLE). Overwrite screen_row after fetch.
        vic.screen_row[0] = 0x00; // BG0
        vic.screen_row[1] = 0x40; // BG1
        vic.screen_row[2] = 0x80; // BG2
        vic.screen_row[3] = 0xC0; // BG3

        // Tick column 0
        tick_vic(&mut vic, &memory);
        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;
        let idx0 = fb_y * FB_WIDTH as usize + fb_x0;
        assert_eq!(
            vic.framebuffer()[idx0],
            PALETTE[0],
            "ECM BG0 should be black"
        );

        // Tick column 1
        tick_vic(&mut vic, &memory);
        let idx1 = fb_y * FB_WIDTH as usize + fb_x0 + 8;
        assert_eq!(vic.framebuffer()[idx1], PALETTE[2], "ECM BG1 should be red");

        // Tick column 2
        tick_vic(&mut vic, &memory);
        let idx2 = fb_y * FB_WIDTH as usize + fb_x0 + 16;
        assert_eq!(
            vic.framebuffer()[idx2],
            PALETTE[5],
            "ECM BG2 should be green"
        );

        // Tick column 3
        tick_vic(&mut vic, &memory);
        let idx3 = fb_y * FB_WIDTH as usize + fb_x0 + 24;
        assert_eq!(
            vic.framebuffer()[idx3],
            PALETTE[6],
            "ECM BG3 should be blue"
        );
    }

    #[test]
    fn mcm_text_bit3_selects_mode() {
        // Chargen: char 0 = alternating bits for easy visual check
        let mut chargen = vec![0x00; 4096];
        chargen[0] = 0b1010_1010; // Char 0, row 0: bits 10 10 10 10

        let memory = TestMemory::new(&chargen);
        let mut vic = Vic::new(VicModel::Pal6569);

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
            tick_vic(&mut vic, &memory);
        }

        // Now at cycle 16 (DISPLAY_START_CYCLE). Overwrite screen/colour rows.
        vic.screen_row[0] = 0; // Char 0
        vic.colour_row[0] = 0x0F; // Bit 3 set → MCM, low 3 bits = 7 (yellow for 11 pair)
        vic.screen_row[1] = 0;
        vic.colour_row[1] = 0x01; // Bit 3 clear → standard text, fg = white

        // Tick column 0 (MCM)
        tick_vic(&mut vic, &memory);
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
        tick_vic(&mut vic, &memory);
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

        let target_line = 100u16;
        let target_cycle = 35u8;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            tick_vic(&mut vic, &memory);
        }

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let base_idx = fb_y * FB_WIDTH as usize;

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

        assert_eq!(
            vic.framebuffer()[base_idx + 198],
            PALETTE[5],
            "MCM pair 10 pixel 0 should be green (sprite colour)"
        );

        assert_eq!(
            vic.framebuffer()[base_idx + 200],
            PALETTE[6],
            "MCM pair 11 pixel 0 should be blue (MC1)"
        );
    }

    #[test]
    fn sprite_sprite_collision() {
        let (mut vic, mut memory) = make_vic_and_memory();

        vic.write(0x15, 0x03); // Enable sprites 0 and 1
        vic.write(0x00, 172); // Sprite 0 X
        vic.write(0x01, 100); // Sprite 0 Y
        vic.write(0x02, 172); // Sprite 1 X (same)
        vic.write(0x03, 100); // Sprite 1 Y (same)
        vic.write(0x27, 0x01); // Sprite 0 colour = white
        vic.write(0x28, 0x02); // Sprite 1 colour = red
        vic.write(0x18, 0x14);
        vic.write(0x11, 0x1B);

        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x07F9, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        let target_line = 100u16;
        let target_cycle = 35u8;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            tick_vic(&mut vic, &memory);
        }

        let collision = vic.read(0x1E);
        assert_eq!(
            collision & 0x03,
            0x03,
            "Sprites 0 and 1 should collide, got {collision:#04X}"
        );

        assert_eq!(vic.read(0x1E), 0x00, "$D01E should be cleared after read");
    }

    #[test]
    fn sprite_bg_collision() {
        let (mut vic, mut memory) = make_vic_and_memory();
        let mut colour_ram = vec![0u8; 1024];

        // chargen is 0xFF (all fg pixels) — sprite overlapping fg triggers collision
        vic.write(0x15, 0x01); // Enable sprite 0
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x18, 0x14); // Screen $0400, chars $1000

        vic.write(0x00, 24); // X = 24 → fb_x = 48 (display window start)
        vic.write(0x01, 51); // Y = 51 (first badline with YSCROLL=3)
        vic.write(0x27, 0x01); // Sprite 0 colour = white

        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        // Set colour RAM to non-zero so char fg is rendered
        colour_ram[0] = 0x01;

        let target_line = 51u16;
        let target_cycle = DISPLAY_START_CYCLE + 1;
        let cycles_to_target =
            u32::from(target_line) * u32::from(CYCLES_PER_LINE) + u32::from(target_cycle);
        for _ in 0..=cycles_to_target {
            tick_vic_with_colour(&mut vic, &memory, &colour_ram);
        }

        let collision = vic.read(0x1F);
        assert_ne!(
            collision & 0x01,
            0x00,
            "Sprite 0 should collide with bg, got {collision:#04X}"
        );
    }

    /// Helper: advance VIC to a specific raster line and cycle.
    fn advance_to(vic: &mut Vic, memory: &TestMemory, line: u16, cycle: u8) {
        let target = u32::from(line) * u32::from(CYCLES_PER_LINE) + u32::from(cycle);
        for _ in 0..target {
            tick_vic(vic, memory);
        }
    }

    /// Helper: get the framebuffer pixel at (`fb_x`, `fb_y`).
    fn fb_pixel(vic: &Vic, fb_x: usize, fb_y: usize) -> u32 {
        vic.framebuffer()[fb_y * FB_WIDTH as usize + fb_x]
    }

    #[test]
    fn xscroll_zero_unchanged() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x16, 0x08); // CSEL=1, XSCROLL=0
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00); // BG = black

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01; // white fg
        tick_vic(&mut vic, &memory); // renders col 0

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;

        for px in 0..8 {
            assert_eq!(
                fb_pixel(&vic, fb_x0 + px, fb_y),
                PALETTE[1],
                "XSCROLL=0: pixel {px} should be white"
            );
        }
    }

    #[test]
    fn xscroll_shifts_right() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x16, 0x0C); // CSEL=1, XSCROLL=4
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00); // BG = black

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01; // white fg
        tick_vic(&mut vic, &memory); // renders col 0

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;

        // Pixels 0-3: background carry (black)
        for px in 0..4 {
            assert_eq!(
                fb_pixel(&vic, fb_x0 + px, fb_y),
                PALETTE[0],
                "XSCROLL=4: pixel {px} should be bg (black)"
            );
        }
        // Pixels 4-7: character fg (white)
        for px in 4..8 {
            assert_eq!(
                fb_pixel(&vic, fb_x0 + px, fb_y),
                PALETTE[1],
                "XSCROLL=4: pixel {px} should be fg (white)"
            );
        }
    }

    #[test]
    fn xscroll_carry_propagates() {
        // Char 0 = 0xFF, char 1 = 0x00
        let mut chargen = vec![0x00; 4096];
        chargen[0] = 0xFF; // Char 0, row 0

        let memory = TestMemory::new(&chargen);
        let mut vic = Vic::new(VicModel::Pal6569);
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x16, 0x0B); // CSEL=1, XSCROLL=3
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00); // BG = black

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.screen_row[0] = 0; // Char 0 (0xFF)
        vic.screen_row[1] = 1; // Char 1 (0x00)
        vic.colour_row[0] = 0x01; // white fg
        vic.colour_row[1] = 0x01; // white fg (won't matter, bitmap is 0x00)

        tick_vic(&mut vic, &memory); // col 0
        tick_vic(&mut vic, &memory); // col 1

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x1 = (DISPLAY_START_CYCLE + 1 - FIRST_VISIBLE_CYCLE) as usize * 8;

        // At col 1: pixels 0-2 are carry from col 0 (rightmost 3 of 0xFF = white)
        for px in 0..3 {
            assert_eq!(
                fb_pixel(&vic, fb_x1 + px, fb_y),
                PALETTE[1],
                "XSCROLL=3 col 1: pixel {px} should be carry (white)"
            );
        }
        // Pixels 3-7 are from char 1 (bitmap 0x00 = bg = black)
        for px in 3..8 {
            assert_eq!(
                fb_pixel(&vic, fb_x1 + px, fb_y),
                PALETTE[0],
                "XSCROLL=3 col 1: pixel {px} should be bg (black)"
            );
        }
    }

    #[test]
    fn xscroll_carry_fg_mask() {
        let (mut vic, mut memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B);
        vic.write(0x16, 0x0A); // CSEL=1, XSCROLL=2
        vic.write(0x18, 0x14);
        vic.write(0x21, 0x00);

        // Enable sprite 0 overlapping col 1, behind fg
        vic.write(0x15, 0x01);
        vic.write(0x1B, 0x01); // Sprite 0 behind fg
        vic.write(0x00, 32);

        let target_line = DISPLAY_START_LINE + 3;
        vic.write(0x01, target_line as u8); // Sprite Y

        memory.ram_write(0x07F8, 0x80);
        memory.ram_write(0x2000, 0xFF);
        memory.ram_write(0x2001, 0xFF);
        memory.ram_write(0x2002, 0xFF);

        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x01;
        vic.colour_row[1] = 0x01;
        tick_vic(&mut vic, &memory); // col 0
        tick_vic(&mut vic, &memory); // col 1

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x1 = (DISPLAY_START_CYCLE + 1 - FIRST_VISIBLE_CYCLE) as usize * 8;

        for px in 0..2 {
            assert_eq!(
                fb_pixel(&vic, fb_x1 + px, fb_y),
                PALETTE[1],
                "XSCROLL carry fg_mask: pixel {px} should be char fg (white), not sprite"
            );
        }
    }

    #[test]
    fn csel_38_column_border() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x1B); // DEN + YSCROLL=3
        vic.write(0x16, 0x00); // CSEL=0, XSCROLL=0
        vic.write(0x18, 0x14);
        vic.write(0x20, 0x06); // Border = blue
        vic.write(0x21, 0x01); // BG = white

        let target_line = DISPLAY_START_LINE + 3;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE);
        vic.colour_row[0] = 0x0E; // yellow fg
        tick_vic(&mut vic, &memory); // col 0 / cycle 16

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x0 = (DISPLAY_START_CYCLE - FIRST_VISIBLE_CYCLE) as usize * 8;

        assert_eq!(
            fb_pixel(&vic, fb_x0, fb_y),
            PALETTE[6], // blue
            "CSEL=0: cycle 16 should show border, not character data"
        );
    }

    #[test]
    fn rsel_24_row_border() {
        let (mut vic, memory) = make_vic_and_memory();
        vic.write(0x11, 0x13); // DEN + YSCROLL=3 + RSEL=0 (bit 3 clear)
        vic.write(0x16, 0x08); // CSEL=1, XSCROLL=0
        vic.write(0x18, 0x14);
        vic.write(0x20, 0x06); // Border = blue
        vic.write(0x21, 0x01); // BG = white

        let target_line = 0x33u16;
        advance_to(&mut vic, &memory, target_line, DISPLAY_START_CYCLE + 5);
        tick_vic(&mut vic, &memory); // renders cycle at column 5

        let fb_y = (target_line - FIRST_VISIBLE_LINE) as usize;
        let fb_x = (DISPLAY_START_CYCLE + 5 - FIRST_VISIBLE_CYCLE) as usize * 8;

        assert_eq!(
            fb_pixel(&vic, fb_x, fb_y),
            PALETTE[6], // blue
            "RSEL=0: line $33 should show border"
        );
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
        assert_eq!(vic.peek(0x14), line as u8, "LPY should match raster line");
        let expected_lpx = (cycle as u16 * 4) as u8;
        assert_eq!(vic.peek(0x13), expected_lpx, "LPX should match cycle * 4");
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
        assert_eq!(
            vic.peek(0x14),
            first_lpy,
            "Second trigger should be ignored"
        );
    }

    #[test]
    fn light_pen_resets_at_frame_start() {
        let (mut vic, memory) = make_vic_and_memory();
        while vic.raster_line() < 50 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();

        while !vic.take_frame_complete() {
            tick_vic(&mut vic, &memory);
        }

        while vic.raster_line() < 100 {
            tick_vic(&mut vic, &memory);
        }
        vic.trigger_light_pen();
        assert_eq!(vic.peek(0x14), 100, "New frame should allow new latch");
    }

    #[test]
    fn unmapped_registers_return_last_bus_data() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..(CYCLES_PER_LINE as u32 * (DISPLAY_START_LINE as u32 + 2)) {
            tick_vic(&mut vic, &memory);
        }
        let val = vic.read(0x2F);
        assert_eq!(
            val,
            vic.peek(0x2F),
            "Read and peek should agree on floating bus value"
        );
    }

    #[test]
    fn unmapped_registers_mirrors_return_same_value() {
        let (mut vic, memory) = make_vic_and_memory();
        for _ in 0..(CYCLES_PER_LINE as u32 * (DISPLAY_START_LINE as u32 + 2)) {
            tick_vic(&mut vic, &memory);
        }
        let val_2f = vic.read(0x2F);
        assert_eq!(vic.read(0x30), val_2f);
        assert_eq!(vic.read(0x3F), val_2f);
    }
}
