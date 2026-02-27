//! Standard Sinclair ULA (Uncommitted Logic Array).
//!
//! The ULA handles video generation, memory contention, keyboard I/O, border
//! colour, and the beeper. This module covers the video and contention aspects;
//! keyboard and beeper are separate concerns wired through the system bus.
//!
//! # Standalone IC
//!
//! This crate has no dependencies — the ULA accesses VRAM through closures
//! passed by the caller, keeping it decoupled from any particular memory model.
//!
//! # Timing (48K PAL)
//!
//! - 448 pixel clocks per line (= 224 CPU T-states x 2)
//! - 312 lines per frame
//! - 139,776 pixel clocks per frame (= 69,888 CPU T-states)
//! - INT asserted for first 64 pixel clocks of frame (= 32 CPU T-states)
//!
//! The ULA is ticked at the 7 MHz pixel clock (once per pixel). All internal
//! state counts in pixel clocks. The methods `tstates_per_line()` and
//! `lines_per_frame()` report in CPU T-states for frame length calculations.
//!
//! # Framebuffer
//!
//! 320x288 pixels: 256 active + 32 left border + 32 right border horizontally,
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
#![allow(clippy::cast_possible_wrap)] // Intentional: u16->i16 for contention offset arithmetic.
#![allow(clippy::cast_sign_loss)] // Intentional: i16->usize after bounds-checking.

mod palette;

pub use palette::PALETTE;

/// Framebuffer dimensions.
pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 288;

/// Display area within the framebuffer.
const BORDER_LEFT: u32 = 32;
const SCREEN_WIDTH: u32 = 256;
const SCREEN_HEIGHT: u32 = 192;

/// Pixel clocks per line (448 = 224 T-states x 2).
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
    /// Snow effect: when the CPU reads display memory during a ULA fetch
    /// phase, the data bus conflict causes the ULA to use the CPU's byte
    /// instead of its own VRAM fetch for the next bitmap read.
    snow_byte: Option<u8>,
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
            snow_byte: None,
        }
    }

    /// Advance the ULA by one pixel clock tick (7 MHz).
    ///
    /// `read_vram` reads a byte from the Spectrum's VRAM without side effects.
    /// It is only called during the active display area for bitmap/attribute
    /// fetches.
    pub fn tick(&mut self, read_vram: impl Fn(u16) -> u8) {
        self.render_pixel(&read_vram);

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

    /// Return contention wait states for a memory access at the current beam
    /// position. `contended` is true when the address falls in contended RAM.
    #[must_use]
    pub fn contention(&self, contended: bool) -> u8 {
        if !contended || !self.in_contention_area() {
            return 0;
        }
        // The CPU's memory access happens at T2. Offset backwards by 2 T-states.
        let offset = self.tstate() as i16 - 2;
        Self::contention_delay_at(offset)
    }

    /// Return contention wait states for an I/O access at the current beam
    /// position.
    ///
    /// `ula_port` is true when the port address has bit 0 clear (ULA port).
    /// `contended_high` is true when the high byte of the port address falls
    /// in contended RAM ($4000-$7FFF).
    #[must_use]
    pub fn io_contention(&self, ula_port: bool, contended_high: bool) -> u8 {
        if !self.in_contention_area() {
            return 0;
        }

        // The I/O cycle is 4 T-states. Contention depends on two factors:
        //   1. Whether the high byte of the port address is in $4000-$7FFF (contended)
        //   2. Whether the port is even (ULA port, bit 0 clear)
        //
        // Four cases, with per-T-state contention applied at the *start* of the
        // I/O operation (T-state offset -1 from the I/O read/write at T3):
        //
        // | High $40-$7F? | Even (ULA)? | Pattern                        |
        // |---------------|-------------|--------------------------------|
        // | No            | Yes         | N:1, C:3                       |
        // | No            | No          | N:4 (no contention)            |
        // | Yes           | Yes         | C:1, C:3                       |
        // | Yes           | No          | C:1, C:1, C:1, C:1            |
        //
        // "N" means no contention applied for that T-state.
        // "C:n" means apply contention at the current beam position, then advance
        //   n T-states before the next contention check.
        //
        // We sum the total contention and apply it all at once.
        let base_offset = self.tstate() as i16 - 1;

        match (contended_high, ula_port) {
            (false, false) => {
                // N:4 -- no contention at all
                0
            }
            (false, true) => {
                // N:1, C:3 -- skip 1, then contend at offset+1
                Self::contention_delay_at(base_offset + 1)
            }
            (true, true) => {
                // C:1, C:3 -- contend at offset, skip 1, contend at offset+1+delay0
                let delay0 = Self::contention_delay_at(base_offset);
                let delay1 = Self::contention_delay_at(base_offset + 1 + i16::from(delay0));
                delay0 + delay1
            }
            (true, false) => {
                // C:1, C:1, C:1, C:1 -- four contention checks at 1-T-state intervals
                let d0 = Self::contention_delay_at(base_offset);
                let d1 = Self::contention_delay_at(base_offset + 1 + i16::from(d0));
                let d2 =
                    Self::contention_delay_at(base_offset + 2 + i16::from(d0) + i16::from(d1));
                let d3 = Self::contention_delay_at(
                    base_offset + 3 + i16::from(d0) + i16::from(d1) + i16::from(d2),
                );
                d0 + d1 + d2 + d3
            }
        }
    }

    /// Is the INT signal currently asserted?
    #[must_use]
    pub fn int_active(&self) -> bool {
        self.line == 0 && self.pixel < INT_LENGTH_PIXELS
    }

    /// Has the frame completed? Auto-clears on read.
    pub fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    /// Total T-states per scanline (224 for all known Sinclair/Timex variants).
    #[must_use]
    pub fn tstates_per_line(&self) -> u16 {
        TSTATES_PER_LINE
    }

    /// Total scanlines per frame (312 for Sinclair/Timex/Scorpion, 320 for Pentagon).
    #[must_use]
    pub fn lines_per_frame(&self) -> u16 {
        LINES_PER_FRAME
    }

    /// Current scanline (0-based).
    #[must_use]
    pub fn line(&self) -> u16 {
        self.line
    }

    /// Current T-state within the current scanline.
    #[must_use]
    pub fn line_tstate(&self) -> u16 {
        self.tstate()
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Current border colour index (0-7).
    #[must_use]
    pub fn border_colour(&self) -> u8 {
        self.border
    }

    /// Set border colour (from port $FE write).
    pub fn set_border_colour(&mut self, colour: u8) {
        self.border = colour & 0x07;
    }

    /// Is the ULA currently fetching screen data (bitmap or attribute)?
    ///
    /// True during the active screen area (lines 64-255) in the first 4
    /// T-states of each 8-T-state fetch group (phases 0-3). A CPU read from
    /// display memory during this window causes the "snow" effect.
    #[must_use]
    pub fn is_screen_fetch_phase(&self) -> bool {
        self.in_contention_area() && (self.tstate() % 8) < 4
    }

    /// Position the ULA at a specific line and T-state (for testing).
    #[doc(hidden)]
    pub fn set_position(&mut self, line: u16, tstate: u16) {
        self.line = line;
        self.pixel = tstate * 2;
    }

    /// Check whether a snow byte is pending (for testing).
    #[doc(hidden)]
    #[must_use]
    pub fn has_snow_byte(&self) -> bool {
        self.snow_byte.is_some()
    }

    /// Inject a snow byte from a bus conflict.
    ///
    /// When the CPU reads display memory ($4000-$5AFF) during a ULA fetch
    /// phase, the shared data bus causes the ULA to latch the CPU's byte.
    /// The next bitmap fetch renders this byte instead of VRAM contents.
    pub fn set_snow_byte(&mut self, byte: u8) {
        self.snow_byte = Some(byte);
    }

    /// Return the floating bus value at the current beam position.
    ///
    /// On a real 48K, unattached port reads leak the ULA's data bus through
    /// 470-ohm resistors. The value depends on what the ULA is fetching:
    ///   T+0: bitmap byte, T+1: attribute byte,
    ///   T+2: bitmap+1 byte, T+3: attribute+1 byte,
    ///   T+4..T+7: $FF (idle).
    /// During border/vblank, returns $FF.
    #[must_use]
    pub fn floating_bus(&self, read_vram: impl Fn(u16) -> u8) -> u8 {
        // Only during the screen area (lines 64-255, T-states 0-127)
        if !self.in_contention_area() {
            return 0xFF;
        }

        let tstate = self.tstate();
        let phase = tstate % 8;

        // ULA fetch pattern within each 8-T-state group:
        //   T+0: bitmap, T+1: attribute, T+2: bitmap+1, T+3: attribute+1
        //   T+4..T+7: idle ($FF)
        if phase >= 4 {
            return 0xFF;
        }

        // Calculate the character column from the T-state.
        // Each 8-T-state group handles 2 character columns (8 pixels = 1 byte x 2).
        let char_col_base = (tstate / 8) * 2;
        let screen_y = (self.line - FIRST_SCREEN_LINE) as u8;

        match phase {
            0 => {
                // Bitmap byte for current column
                Self::bitmap_addr(screen_y, char_col_base as u8)
                    .map_or(0xFF, &read_vram)
            }
            1 => {
                // Attribute byte for current column
                Self::attr_addr(screen_y, char_col_base as u8)
                    .map_or(0xFF, &read_vram)
            }
            2 => {
                // Bitmap byte for next column
                let col = char_col_base + 1;
                if col >= 32 {
                    return 0xFF;
                }
                Self::bitmap_addr(screen_y, col as u8).map_or(0xFF, &read_vram)
            }
            3 => {
                // Attribute byte for next column
                let col = char_col_base + 1;
                if col >= 32 {
                    return 0xFF;
                }
                Self::attr_addr(screen_y, col as u8).map_or(0xFF, &read_vram)
            }
            _ => 0xFF,
        }
    }

    // === Internal helpers ===

    /// Current CPU T-state within the line (pixel / 2).
    fn tstate(&self) -> u16 {
        self.pixel / 2
    }

    /// Render one pixel for the current beam position.
    fn render_pixel(&mut self, read_vram: &impl Fn(u16) -> u8) {
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
            // Horizontal retrace -- not visible
            return;
        };

        // Map the current line to a framebuffer y coordinate.
        // Lines 16-63: top border -> fb_y 0..48
        // Lines 64-255: screen -> fb_y 48..240
        // Lines 256-303: bottom border -> fb_y 240..288
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
            self.render_screen_pixel(read_vram, fb_x, fb_y, line, pixel);
        } else {
            self.render_border_pixel(fb_x, fb_y);
        }
    }

    /// Render 1 screen pixel.
    fn render_screen_pixel(
        &mut self,
        read_vram: &impl Fn(u16) -> u8,
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

        // Snow effect: if a bus conflict injected a byte, use it instead of
        // the VRAM bitmap fetch. The attribute byte is unaffected.
        let bitmap = if let Some(snow) = self.snow_byte.take() {
            snow
        } else {
            read_vram(bitmap_addr)
        };
        let attr = read_vram(attr_addr);

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

    /// Compute the bitmap address for a given screen Y and character column.
    fn bitmap_addr(screen_y: u8, char_col: u8) -> Option<u16> {
        if char_col >= 32 {
            return None;
        }
        let y7y6 = (screen_y >> 6) & 0x03;
        let y5y4y3 = (screen_y >> 3) & 0x07;
        let y2y1y0 = screen_y & 0x07;
        Some(
            0x4000
                | (u16::from(y7y6) << 11)
                | (u16::from(y2y1y0) << 8)
                | (u16::from(y5y4y3) << 5)
                | u16::from(char_col),
        )
    }

    /// Compute the attribute address for a given screen Y and character column.
    fn attr_addr(screen_y: u8, char_col: u8) -> Option<u16> {
        if char_col >= 32 {
            return None;
        }
        Some(0x5800 | (u16::from(screen_y / 8) << 5) | u16::from(char_col))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple 64K memory for tests: 16K ROM (zeros) + 48K RAM.
    struct TestMemory {
        data: [u8; 0x10000],
    }

    impl TestMemory {
        fn new() -> Self {
            Self {
                data: [0; 0x10000],
            }
        }

        fn peek(&self, addr: u16) -> u8 {
            self.data[addr as usize]
        }

        fn write(&mut self, addr: u16, val: u8) {
            self.data[addr as usize] = val;
        }

    }

    #[test]
    fn frame_timing_pixel_clocks() {
        let mut ula = Ula::new();
        let mem = TestMemory::new();
        let total_pixels = u32::from(PIXELS_PER_LINE) * u32::from(LINES_PER_FRAME);
        assert_eq!(total_pixels, 139_776); // 448 x 312

        // Tick through one complete frame at pixel clock rate
        for _ in 0..total_pixels {
            assert!(!ula.frame_complete, "frame_complete set too early");
            ula.tick(|addr| mem.peek(addr));
        }
        assert!(ula.take_frame_complete());
        assert!(
            !ula.take_frame_complete(),
            "take_frame_complete should auto-clear"
        );
    }

    #[test]
    fn frame_timing_matches_tstates() {
        // 448 pixels/line / 2 = 224 T-states/line
        // 224 T-states x 312 lines = 69,888 T-states per frame
        let tstates = u32::from(TSTATES_PER_LINE) * u32::from(LINES_PER_FRAME);
        assert_eq!(tstates, 69_888);
    }

    #[test]
    fn int_timing() {
        let mut ula = Ula::new();
        let mem = TestMemory::new();

        assert!(ula.int_active());

        // Tick through INT period (64 pixel clocks = 32 T-states)
        for _ in 0..INT_LENGTH_PIXELS {
            assert!(ula.int_active());
            ula.tick(|addr| mem.peek(addr));
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
        let mem = TestMemory::new();
        let pixels_per_frame = u32::from(PIXELS_PER_LINE) * u32::from(LINES_PER_FRAME);

        assert!(!ula.flash_state);

        for _ in 0..16 {
            for _ in 0..pixels_per_frame {
                ula.tick(|addr| mem.peek(addr));
            }
        }
        assert!(ula.flash_state);

        for _ in 0..16 {
            for _ in 0..pixels_per_frame {
                ula.tick(|addr| mem.peek(addr));
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
        let mem = TestMemory::new();
        assert_eq!(ula.line_tstate(), 0);

        // Tick 2 pixel clocks = 1 CPU T-state
        ula.tick(|addr| mem.peek(addr));
        ula.tick(|addr| mem.peek(addr));
        assert_eq!(ula.line_tstate(), 1);
    }

    // === Contention tests ===

    /// Position the ULA at a specific line and T-state.
    fn position_ula(ula: &mut Ula, line: u16, tstate: u16) {
        ula.line = line;
        ula.pixel = tstate * 2;
    }

    #[test]
    fn contention_in_screen_area() {
        let mut ula = Ula::new();

        // Line 64 (first screen line), T-state 2 (offset 0 after -2 adjustment)
        position_ula(&mut ula, 64, 2);
        // Offset 0 -> pattern[0] = 6
        assert_eq!(ula.contention(true), 6);

        // T-state 3 -> offset 1 -> pattern[1] = 5
        position_ula(&mut ula, 64, 3);
        assert_eq!(ula.contention(true), 5);

        // T-state 8 -> offset 6 -> pattern[6] = 0
        position_ula(&mut ula, 64, 8);
        assert_eq!(ula.contention(true), 0);

        // T-state 9 -> offset 7 -> pattern[7] = 0
        position_ula(&mut ula, 64, 9);
        assert_eq!(ula.contention(true), 0);
    }

    #[test]
    fn contention_outside_screen_area() {
        let mut ula = Ula::new();

        // Line 0 (vblank) -- no contention
        position_ula(&mut ula, 0, 2);
        assert_eq!(ula.contention(true), 0);

        // Line 256 (bottom border) -- no contention
        position_ula(&mut ula, 256, 2);
        assert_eq!(ula.contention(true), 0);

        // Line 64 but T-state 128+ (beyond contention window)
        position_ula(&mut ula, 64, 130);
        assert_eq!(ula.contention(true), 0);
    }

    #[test]
    fn contention_non_contended_ram() {
        let mut ula = Ula::new();

        // Contended area, but address is not contended
        position_ula(&mut ula, 64, 2);
        assert_eq!(ula.contention(false), 0);
    }

    #[test]
    fn io_contention_no_contended_no_ula() {
        let mut ula = Ula::new();

        // Port $01FF -- high byte $01 (not contended), odd (not ULA)
        // Pattern: N:4 -> 0 contention
        position_ula(&mut ula, 64, 2);
        assert_eq!(ula.io_contention(false, false), 0);
    }

    #[test]
    fn io_contention_no_contended_ula() {
        let mut ula = Ula::new();

        // Port $00FE -- high byte $00 (not contended), even (ULA)
        // Pattern: N:1, C:3 -> contention at offset+1
        position_ula(&mut ula, 64, 2);
        let delay = ula.io_contention(true, false);
        // base_offset = 2 - 1 = 1, check at offset 2 -> pattern[2%8] = 4
        assert_eq!(delay, 4);
    }

    #[test]
    fn io_contention_contended_ula() {
        let mut ula = Ula::new();

        // Port $40FE -- high byte $40 (contended), even (ULA)
        // Pattern: C:1, C:3
        position_ula(&mut ula, 64, 2);
        let delay = ula.io_contention(true, true);
        // base_offset = 1
        // d0 = pattern[1%8] = 5
        // d1 = pattern[(2+5)%8] = pattern[7%8] = 0
        assert_eq!(delay, 5);
    }

    #[test]
    fn io_contention_contended_not_ula() {
        let mut ula = Ula::new();

        // Port $40FF -- high byte $40 (contended), odd (not ULA)
        // Pattern: C:1, C:1, C:1, C:1
        position_ula(&mut ula, 64, 2);
        let delay = ula.io_contention(false, true);
        // base_offset = 1
        // d0 = pattern[1] = 5
        // d1 = pattern[1+1+5] = pattern[7] = 0
        // d2 = pattern[1+2+5+0] = pattern[0] = 6
        // d3 = pattern[1+3+5+0+6] = pattern[15%8] = pattern[7] = 0
        assert_eq!(delay, 5 + 0 + 6 + 0);
    }

    #[test]
    fn io_contention_outside_screen() {
        let mut ula = Ula::new();

        // During border -- no contention regardless of port
        position_ula(&mut ula, 0, 2);
        assert_eq!(ula.io_contention(true, true), 0);
        assert_eq!(ula.io_contention(false, true), 0);
        assert_eq!(ula.io_contention(true, false), 0);
    }

    // === Floating bus tests ===

    #[test]
    fn floating_bus_during_border() {
        let ula = Ula::new(); // Line 0 = vblank
        let mem = TestMemory::new();
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0xFF);
    }

    #[test]
    fn floating_bus_idle_phase() {
        let mut ula = Ula::new();
        let mem = TestMemory::new();

        // Position at screen line 64, T-state 4 (phase 4 = idle)
        position_ula(&mut ula, 64, 4);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0xFF);

        // Phase 5, 6, 7 are also idle
        position_ula(&mut ula, 64, 5);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0xFF);
    }

    #[test]
    fn floating_bus_bitmap_fetch() {
        let mut ula = Ula::new();
        let mut mem = TestMemory::new();

        // Position at line 64, T-state 0 (phase 0 = bitmap, column 0)
        position_ula(&mut ula, 64, 0);
        // Bitmap address for screen_y=0, char_col=0:
        // 010 00 000 000 00000 = $4000
        mem.write(0x4000, 0xAA);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0xAA);
    }

    #[test]
    fn floating_bus_attribute_fetch() {
        let mut ula = Ula::new();
        let mut mem = TestMemory::new();

        // Position at line 64, T-state 1 (phase 1 = attribute, column 0)
        position_ula(&mut ula, 64, 1);
        // Attribute address for screen_y=0, char_col=0 = $5800
        mem.write(0x5800, 0x38);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0x38);
    }

    #[test]
    fn floating_bus_second_column() {
        let mut ula = Ula::new();
        let mut mem = TestMemory::new();

        // Position at line 64, T-state 2 (phase 2 = bitmap+1, column 1)
        position_ula(&mut ula, 64, 2);
        // Bitmap address for screen_y=0, char_col=1 = $4001
        mem.write(0x4001, 0x55);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0x55);

        // T-state 3 (phase 3 = attribute+1, column 1)
        position_ula(&mut ula, 64, 3);
        // Attribute address for screen_y=0, char_col=1 = $5801
        mem.write(0x5801, 0x47);
        assert_eq!(ula.floating_bus(|addr| mem.peek(addr)), 0x47);
    }

    // === Snow effect tests ===

    #[test]
    fn snow_byte_replaces_bitmap_fetch() {
        let mut ula = Ula::new();
        let mut mem = TestMemory::new();

        // Set up normal VRAM: bitmap=$4000=0x00, attr=$5800=0x38 (paper=7, ink=0)
        mem.write(0x4000, 0x00);
        mem.write(0x5800, 0x38);

        // Position at line 64, pixel 0 (first screen pixel, column 0)
        position_ula(&mut ula, 64, 0);
        ula.pixel = 0; // Exact pixel position for rendering

        // Inject snow byte (all pixels set)
        ula.set_snow_byte(0xFF);

        // Tick once — this renders pixel 0 of the first screen character
        ula.tick(|addr| mem.peek(addr));

        // The pixel should use the snow byte (0xFF = all ink) not VRAM (0x00 = all paper).
        // With attr 0x38: ink=0 (black), paper=7 (white). Snow byte 0xFF means bit 7=1 → ink.
        let fb_idx = (48 * FB_WIDTH + BORDER_LEFT) as usize; // line 64→fb_y 48
        let black = PALETTE[0]; // ink=0
        assert_eq!(ula.framebuffer()[fb_idx], black, "snow byte should produce ink colour");
    }

    #[test]
    fn is_screen_fetch_phase_true_during_fetch() {
        let mut ula = Ula::new();
        // Line 64, T-state 0: phase 0 of the fetch group
        position_ula(&mut ula, 64, 0);
        assert!(ula.is_screen_fetch_phase());

        // T-state 3: still in fetch (phases 0-3)
        position_ula(&mut ula, 64, 3);
        assert!(ula.is_screen_fetch_phase());
    }

    #[test]
    fn is_screen_fetch_phase_false_during_idle() {
        let mut ula = Ula::new();
        // Line 64, T-state 4: idle phase
        position_ula(&mut ula, 64, 4);
        assert!(!ula.is_screen_fetch_phase());

        // T-state 7: still idle
        position_ula(&mut ula, 64, 7);
        assert!(!ula.is_screen_fetch_phase());
    }

    #[test]
    fn is_screen_fetch_phase_false_during_border() {
        let mut ula = Ula::new();
        // Line 0 (vblank): not screen area at all
        position_ula(&mut ula, 0, 0);
        assert!(!ula.is_screen_fetch_phase());

        // Line 300 (bottom border): not screen area
        position_ula(&mut ula, 300, 0);
        assert!(!ula.is_screen_fetch_phase());
    }

    #[test]
    fn snow_byte_clears_after_use() {
        let mut ula = Ula::new();
        let mem = TestMemory::new();

        // Position at first screen pixel
        position_ula(&mut ula, 64, 0);
        ula.pixel = 0;

        ula.set_snow_byte(0xFF);
        assert!(ula.snow_byte.is_some());

        // Tick once — renders the pixel and consumes the snow byte
        ula.tick(|addr| mem.peek(addr));
        assert!(ula.snow_byte.is_none(), "snow_byte should be consumed after rendering");
    }
}
