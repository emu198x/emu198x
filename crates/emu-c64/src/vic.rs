//! VIC-II 6569 (PAL) video chip.
//!
//! Implements text mode rendering, raster counter, raster IRQ, and badline
//! cycle stealing. Sprites, bitmap mode, and multicolour mode are not
//! implemented in v1.
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
        }
    }

    /// Tick the VIC-II for one CPU cycle.
    ///
    /// Renders 8 pixels, advances the beam, detects badlines.
    /// Returns `true` if the CPU should be stalled this cycle (badline DMA).
    pub fn tick(&mut self, memory: &C64Memory) -> bool {
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

        for col in 0u16..40 {
            let screen_addr = screen_base + text_row * 40 + col;
            // VIC-II reads through its own bus (sees char ROM, not I/O)
            self.screen_row[col as usize] = memory.vic_read(self.vic_bank, screen_addr & 0x3FFF);
            self.colour_row[col as usize] = memory.colour_ram_read(text_row * 40 + col);
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

        if !in_display {
            // Border area
            for px in 0..8usize {
                let idx = fb_offset + px;
                if idx < self.framebuffer.len() {
                    self.framebuffer[idx] = border_colour;
                }
            }
            return;
        }

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
            return;
        }

        let char_code = self.screen_row[col];
        let fg_colour = PALETTE[(self.colour_row[col] & 0x0F) as usize];

        // Fetch the character bitmap for the current row
        let char_base = self.char_base();
        let bitmap_addr = char_base + u16::from(char_code) * 8 + u16::from(self.char_row);
        let bitmap = memory.vic_read(self.vic_bank, bitmap_addr & 0x3FFF);

        // Render 8 pixels
        for px in 0..8usize {
            let bit = (bitmap >> (7 - px)) & 1;
            let colour = if bit != 0 { fg_colour } else { bg_colour };
            let idx = fb_offset + px;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = colour;
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

    /// Read a VIC-II register.
    #[must_use]
    pub fn read(&self, reg: u8) -> u8 {
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
            r if r <= 0x2E => self.regs[r as usize],
            // Unused registers return $FF
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
}
