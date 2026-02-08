//! Standard Sinclair ULA (Uncommitted Logic Array).
//!
//! The ULA handles video generation, memory contention, keyboard I/O, border
//! colour, and the beeper. This module covers the video and contention aspects;
//! keyboard and beeper are separate modules wired through the bus.
//!
//! # Timing (48K PAL)
//!
//! - 448 pixel clocks per line (= 224 CPU T-states × 2)
//! - 312 lines per frame
//! - 139,776 pixel clocks per frame (= 69,888 CPU T-states)
//! - INT asserted for first 64 pixel clocks of frame (= 32 CPU T-states)
//!
//! The ULA is ticked at the 7 MHz pixel clock (once per pixel). All internal
//! state counts in pixel clocks. The trait methods `tstates_per_line()` and
//! `lines_per_frame()` report in CPU T-states for frame length calculations.
//!
//! # Framebuffer
//!
//! 320×288 pixels: 256 active + 32 left border + 32 right border horizontally,
//! 192 active + 48 top border + 48 bottom border vertically.
//!
//! # Screen memory layout
//!
//! Bitmap at $4000-$57FF (6144 bytes), attributes at $5800-$5AFF (768 bytes).
//! Bitmap address: `010Y7 Y6Y2 Y1Y0 Y5Y4Y3 X4X3X2X1X0`
//! Attribute address: `0101 10Y7 Y6Y5 Y4Y3 X4X3X2X1X0`
//!
//! # Contention
//!
//! During screen fetch (lines 64-255, T-states 0-127), the ULA contends
//! memory access. Pattern repeats every 8 T-states: `[6, 5, 4, 3, 2, 1, 0, 0]`.
//! Contention is reported in CPU T-states, using the pixel clock position
//! divided by 2 to derive the CPU T-state.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)] // Intentional: u16→i16 for contention offset arithmetic.
#![allow(clippy::cast_sign_loss)] // Intentional: i16→usize after bounds-checking.

use crate::memory::SpectrumMemory;
use crate::palette::PALETTE;
use crate::video::SpectrumVideo;

/// Framebuffer dimensions.
const FB_WIDTH: u32 = 320;
const FB_HEIGHT: u32 = 288;

/// Display area within the framebuffer.
const BORDER_LEFT: u32 = 32;
const SCREEN_WIDTH: u32 = 256;
const SCREEN_HEIGHT: u32 = 192;

/// Pixel clocks per line (448 = 224 T-states × 2).
const PIXELS_PER_LINE: u16 = 448;
/// Lines per frame.
const LINES_PER_FRAME: u16 = 312;
/// CPU T-states per line (224).
const TSTATES_PER_LINE: u16 = 224;

/// INT is asserted for the first 64 pixel clocks (= 32 CPU T-states).
const INT_LENGTH_PIXELS: u16 = 64;

/// First screen line (after top border). Line 0 is the start of the frame,
/// which begins at the top of the vertical retrace. The top border starts
/// at line 64 - 48 = 16 and the screen area starts at line 64.
const FIRST_SCREEN_LINE: u16 = 64;

/// Contention area in CPU T-states: 0-127.
const CONTENTION_END_TSTATE: u16 = 128;

/// Contention delay pattern (repeats every 8 T-states).
const CONTENTION_PATTERN: [u8; 8] = [6, 5, 4, 3, 2, 1, 0, 0];

/// Number of frames between FLASH toggles.
const FLASH_FRAME_COUNT: u8 = 16;

/// Standard Sinclair ULA.
pub struct Ula {
    /// Current scanline (0 = start of frame).
    line: u16,
    /// Current pixel clock within the line (0-447).
    pixel: u16,
    /// Frame complete flag, auto-clears on read.
    frame_complete: bool,
    /// Current border colour (0-7).
    border: u8,
    /// FLASH state: false = normal, true = inverted.
    flash_state: bool,
    /// Frame counter for FLASH timing.
    flash_counter: u8,
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
}

impl Ula {
    #[must_use]
    pub fn new() -> Self {
        Self {
            line: 0,
            pixel: 0,
            frame_complete: false,
            border: 7, // White border on power-up
            flash_state: false,
            flash_counter: 0,
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
        }
    }

    /// Current CPU T-state within the line (pixel / 2).
    fn tstate(&self) -> u16 {
        self.pixel / 2
    }

    /// Render one pixel for the current beam position.
    fn render_pixel(&mut self, memory: &dyn SpectrumMemory) {
        let line = self.line;
        let pixel = self.pixel;

        // Map the current pixel clock to a framebuffer x coordinate.
        //
        // ULA horizontal timing (in pixel clocks, 448 total):
        //   Pixels   0-255: screen fetch area (256 pixels)
        //   Pixels 256-287: right border (32 pixels)
        //   Pixels 288-415: horizontal retrace (not visible)
        //   Pixels 416-447: left border (32 pixels)
        let (fb_x, in_screen_area) = if pixel < 256 {
            // Screen area
            (BORDER_LEFT + u32::from(pixel), true)
        } else if pixel < 288 {
            // Right border
            (BORDER_LEFT + SCREEN_WIDTH + u32::from(pixel - 256), false)
        } else if pixel >= 416 {
            // Left border
            (u32::from(pixel - 416), false)
        } else {
            // Horizontal retrace — not visible
            return;
        };

        // Map the current line to a framebuffer y coordinate.
        // Lines 16-63: top border → fb_y 0..48
        // Lines 64-255: screen → fb_y 48..240
        // Lines 256-303: bottom border → fb_y 240..288
        // Lines 304-311: vertical retrace (not visible)
        // Lines 0-15: vertical sync / not visible
        let fb_y = if (16..304).contains(&line) {
            u32::from(line - 16)
        } else {
            return;
        };

        if fb_y >= FB_HEIGHT {
            return;
        }

        // Is this pixel in the active screen area?
        let in_screen = in_screen_area
            && line >= FIRST_SCREEN_LINE
            && line < FIRST_SCREEN_LINE + SCREEN_HEIGHT as u16;

        if in_screen {
            self.render_screen_pixel(memory, fb_x, fb_y, line, pixel);
        } else {
            self.render_border_pixel(fb_x, fb_y);
        }
    }

    /// Render 1 screen pixel.
    fn render_screen_pixel(
        &mut self,
        memory: &dyn SpectrumMemory,
        fb_x: u32,
        fb_y: u32,
        line: u16,
        pixel: u16,
    ) {
        let screen_y = (line - FIRST_SCREEN_LINE) as u8;
        let pixel_x = pixel as u8; // 0-255 within the screen area

        // Which character column (0-31) and pixel within byte (0-7)?
        let char_col = pixel_x / 8;
        let bit_pos = 7 - (pixel_x % 8);

        // Bitmap address: 010Y7 Y6Y2 Y1Y0 Y5Y4Y3 X4X3X2X1X0
        let y7y6 = (screen_y >> 6) & 0x03;
        let y5y4y3 = (screen_y >> 3) & 0x07;
        let y2y1y0 = screen_y & 0x07;
        let bitmap_addr: u16 = 0x4000
            | (u16::from(y7y6) << 11)
            | (u16::from(y2y1y0) << 8)
            | (u16::from(y5y4y3) << 5)
            | u16::from(char_col);

        // Attribute address: 0101 10Y7 Y6Y5 Y4Y3 X4X3X2X1X0
        let attr_addr: u16 = 0x5800 | (u16::from(screen_y / 8) << 5) | u16::from(char_col);

        let bitmap = memory.peek(bitmap_addr);
        let attr = memory.peek(attr_addr);

        // Decode attribute byte: FBPPPIII
        let flash = attr & 0x80 != 0;
        let bright = attr & 0x40 != 0;
        let paper = (attr >> 3) & 0x07;
        let ink = attr & 0x07;

        let (fg, bg) = if flash && self.flash_state {
            (paper, ink)
        } else {
            (ink, paper)
        };

        let bright_offset: u8 = if bright { 8 } else { 0 };
        let fg_colour = PALETTE[(fg + bright_offset) as usize];
        let bg_colour = PALETTE[(bg + bright_offset) as usize];

        let colour = if bitmap & (1 << bit_pos) != 0 {
            fg_colour
        } else {
            bg_colour
        };

        self.framebuffer[(fb_y * FB_WIDTH + fb_x) as usize] = colour;
    }

    /// Render 1 border pixel.
    fn render_border_pixel(&mut self, fb_x: u32, fb_y: u32) {
        self.framebuffer[(fb_y * FB_WIDTH + fb_x) as usize] = PALETTE[self.border as usize];
    }

    /// Is the current beam position within the contention area?
    fn in_contention_area(&self) -> bool {
        let tstate = self.tstate();
        self.line >= FIRST_SCREEN_LINE
            && self.line < FIRST_SCREEN_LINE + SCREEN_HEIGHT as u16
            && tstate < CONTENTION_END_TSTATE
    }

    /// Look up contention delay for a given CPU T-state offset within the line.
    fn contention_delay_at(tstate_offset: i16) -> u8 {
        if tstate_offset < 0 || tstate_offset >= CONTENTION_END_TSTATE as i16 {
            return 0;
        }
        CONTENTION_PATTERN[tstate_offset as usize % 8]
    }
}

impl Default for Ula {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumVideo for Ula {
    fn tick(&mut self, memory: &dyn SpectrumMemory) {
        self.render_pixel(memory);

        // Advance beam position (pixel clock)
        self.pixel += 1;
        if self.pixel >= PIXELS_PER_LINE {
            self.pixel = 0;
            self.line += 1;
            if self.line >= LINES_PER_FRAME {
                self.line = 0;
                self.frame_complete = true;
                self.flash_counter += 1;
                if self.flash_counter >= FLASH_FRAME_COUNT {
                    self.flash_counter = 0;
                    self.flash_state = !self.flash_state;
                }
            }
        }
    }

    fn contention(&self, addr: u16, memory: &dyn SpectrumMemory) -> u8 {
        if !memory.contended_page(addr) || !self.in_contention_area() {
            return 0;
        }
        // The CPU's memory access happens at T2. Offset backwards by 2 T-states.
        let offset = self.tstate() as i16 - 2;
        Self::contention_delay_at(offset)
    }

    fn io_contention(&self, port: u16, memory: &dyn SpectrumMemory) -> u8 {
        let ula_port = port & 1 == 0;
        let contended_addr = memory.contended_page(port);

        if !self.in_contention_area() {
            return 0;
        }

        if ula_port || contended_addr {
            let offset = self.tstate() as i16 - 1;
            Self::contention_delay_at(offset)
        } else {
            0
        }
    }

    fn int_active(&self) -> bool {
        self.line == 0 && self.pixel < INT_LENGTH_PIXELS
    }

    fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    fn tstates_per_line(&self) -> u16 {
        TSTATES_PER_LINE
    }

    fn lines_per_frame(&self) -> u16 {
        LINES_PER_FRAME
    }

    fn line(&self) -> u16 {
        self.line
    }

    fn line_tstate(&self) -> u16 {
        self.tstate()
    }

    fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    fn border_colour(&self) -> u8 {
        self.border
    }

    fn set_border_colour(&mut self, colour: u8) {
        self.border = colour & 0x07;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory48K;

    fn make_memory() -> Memory48K {
        Memory48K::new(&vec![0u8; 0x4000])
    }

    #[test]
    fn frame_timing_pixel_clocks() {
        let mut ula = Ula::new();
        let memory = make_memory();
        let total_pixels = u32::from(PIXELS_PER_LINE) * u32::from(LINES_PER_FRAME);
        assert_eq!(total_pixels, 139_776); // 448 × 312

        // Tick through one complete frame at pixel clock rate
        for _ in 0..total_pixels {
            assert!(!ula.frame_complete, "frame_complete set too early");
            ula.tick(&memory);
        }
        assert!(ula.take_frame_complete());
        assert!(!ula.take_frame_complete(), "take_frame_complete should auto-clear");
    }

    #[test]
    fn frame_timing_matches_tstates() {
        // 448 pixels/line / 2 = 224 T-states/line
        // 224 T-states × 312 lines = 69,888 T-states per frame
        let tstates = u32::from(TSTATES_PER_LINE) * u32::from(LINES_PER_FRAME);
        assert_eq!(tstates, 69_888);
    }

    #[test]
    fn int_timing() {
        let mut ula = Ula::new();
        let memory = make_memory();

        assert!(ula.int_active());

        // Tick through INT period (64 pixel clocks = 32 T-states)
        for _ in 0..INT_LENGTH_PIXELS {
            assert!(ula.int_active());
            ula.tick(&memory);
        }
        assert!(!ula.int_active());
    }

    #[test]
    fn border_colour() {
        let mut ula = Ula::new();
        assert_eq!(ula.border_colour(), 7);

        ula.set_border_colour(2);
        assert_eq!(ula.border_colour(), 2);

        ula.set_border_colour(0xFF);
        assert_eq!(ula.border_colour(), 7);
    }

    #[test]
    fn flash_toggles_every_16_frames() {
        let mut ula = Ula::new();
        let memory = make_memory();
        let pixels_per_frame = u32::from(PIXELS_PER_LINE) * u32::from(LINES_PER_FRAME);

        assert!(!ula.flash_state);

        for _ in 0..16 {
            for _ in 0..pixels_per_frame {
                ula.tick(&memory);
            }
        }
        assert!(ula.flash_state);

        for _ in 0..16 {
            for _ in 0..pixels_per_frame {
                ula.tick(&memory);
            }
        }
        assert!(!ula.flash_state);
    }

    #[test]
    fn tstates_per_line_reports_cpu_tstates() {
        let ula = Ula::new();
        assert_eq!(ula.tstates_per_line(), 224);
    }

    #[test]
    fn line_tstate_reports_cpu_tstates() {
        let mut ula = Ula::new();
        let memory = make_memory();
        assert_eq!(ula.line_tstate(), 0);

        // Tick 2 pixel clocks = 1 CPU T-state
        ula.tick(&memory);
        ula.tick(&memory);
        assert_eq!(ula.line_tstate(), 1);
    }
}
