//! MC6847 Video Display Generator (VDG).
//!
//! The MC6847 generates video output for the Acorn Atom. It supports text
//! (32x16 characters) and graphics modes (up to 256x192). For v1, only
//! text mode is implemented.
//!
//! The VDG reads character data from RAM ($8000-$83FF on the Atom, but
//! the machine provides the read callback). Each character is 8x12 pixels
//! using an internal character ROM.
//!
//! Display timing: 256 visible pixels/line, 192 visible lines (text: 32
//! columns x 16 rows x 12 scanlines). Total ~262 lines (NTSC) or ~312
//! lines (PAL). The Atom uses the PAL version.

/// Framebuffer dimensions.
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 192;

/// MC6847 Video Display Generator.
pub struct Mc6847 {
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// VDG control register (AG, AS, CSS, INV, etc.).
    pub control: u8,
    /// Whether a frame has completed.
    frame_complete: bool,
    /// Current scanline counter.
    scanline: u32,
    /// Current pixel position within the scanline.
    pixel_x: u32,
    /// Total ticks this frame.
    ticks_in_frame: u32,
}

/// PAL line count for the Atom's MC6847.
const TOTAL_LINES: u32 = 312;
/// Characters per line for the Atom's MC6847.
const TICKS_PER_LINE: u32 = 228;

impl Mc6847 {
    /// Create a new VDG.
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            control: 0,
            frame_complete: false,
            scanline: 0,
            pixel_x: 0,
            ticks_in_frame: 0,
        }
    }

    /// Tick one VDG cycle. Call with a memory read callback for fetching
    /// video RAM. Returns true at the start of a new frame.
    pub fn tick(&mut self, read_video_ram: impl Fn(u16) -> u8) -> bool {
        self.ticks_in_frame += 1;
        self.pixel_x += 1;

        if self.pixel_x >= TICKS_PER_LINE {
            self.pixel_x = 0;
            self.scanline += 1;

            if self.scanline >= TOTAL_LINES {
                self.scanline = 0;
                self.frame_complete = true;
                return true;
            }
        }

        // Render during the visible area
        // Visible lines: scanlines 24..216 (192 visible lines)
        let visible_start = 24;
        let visible_end = visible_start + 192;

        if self.scanline >= visible_start
            && self.scanline < visible_end
            && self.pixel_x < 256
        {
            let vis_y = self.scanline - visible_start;
            let vis_x = self.pixel_x;

            let pixel = if self.control & 0x80 == 0 {
                // Text mode (alphanumeric): 32x16 characters, 8x12 each
                self.render_text_pixel(vis_x, vis_y, &read_video_ram)
            } else {
                // Graphics mode stub: green for now
                0xFF00_8000
            };

            let idx = (vis_y * FB_WIDTH + vis_x) as usize;
            if idx < self.framebuffer.len() {
                self.framebuffer[idx] = pixel;
            }
        }

        false
    }

    /// Render one pixel in text mode.
    fn render_text_pixel(
        &self,
        x: u32,
        y: u32,
        read_video_ram: &impl Fn(u16) -> u8,
    ) -> u32 {
        let char_col = x / 8;
        let char_row = y / 12;
        let pixel_in_char_x = x % 8;
        let pixel_in_char_y = y % 12;

        // Read character code from video RAM
        let char_addr = (char_row * 32 + char_col) as u16;
        let char_code = read_video_ram(char_addr);

        // Look up pixel from internal character ROM
        let inverted = char_code & 0x80 != 0;
        let char_index = (char_code & 0x3F) as usize;
        let row_data = CHAR_ROM[char_index * 12 + pixel_in_char_y as usize];
        let bit = (row_data >> (7 - pixel_in_char_x)) & 1;

        let fg = if inverted { bit == 0 } else { bit != 0 };

        // Green phosphor text
        if fg {
            0xFF00_FF00 // Green on black
        } else {
            0xFF00_0000 // Black
        }
    }

    /// Take the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
    }

    /// Reference to the framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }
}

impl Default for Mc6847 {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal 5x7 character set embedded in the MC6847 ROM, stored as 12 rows
// per character (top 2 and bottom 3 rows blank for spacing within the 8x12
// cell). 64 characters: space, punctuation, digits, uppercase A-Z.
static CHAR_ROM: [u8; 64 * 12] = {
    let mut rom = [0u8; 64 * 12];

    // Helper: sets rows 2..9 (the 7 active rows out of 12) for a character
    macro_rules! char_data {
        ($idx:expr, $r0:expr, $r1:expr, $r2:expr, $r3:expr, $r4:expr, $r5:expr, $r6:expr) => {
            let base = $idx * 12;
            rom[base + 2] = $r0;
            rom[base + 3] = $r1;
            rom[base + 4] = $r2;
            rom[base + 5] = $r3;
            rom[base + 6] = $r4;
            rom[base + 7] = $r5;
            rom[base + 8] = $r6;
        };
    }

    // Space (0x20 mapped to index 0)
    char_data!(0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00);
    // ! (index 1)
    char_data!(1, 0x10, 0x10, 0x10, 0x10, 0x00, 0x10, 0x00);
    // A-Z at indices 33-58 (0x41-0x5A mapped to indices 0x41-0x20=33..58)
    // A
    char_data!(33, 0x38, 0x44, 0x44, 0x7C, 0x44, 0x44, 0x44);
    // B
    char_data!(34, 0x78, 0x44, 0x78, 0x44, 0x44, 0x44, 0x78);
    // C
    char_data!(35, 0x38, 0x44, 0x40, 0x40, 0x40, 0x44, 0x38);
    // D
    char_data!(36, 0x78, 0x44, 0x44, 0x44, 0x44, 0x44, 0x78);
    // E
    char_data!(37, 0x7C, 0x40, 0x78, 0x40, 0x40, 0x40, 0x7C);
    // F
    char_data!(38, 0x7C, 0x40, 0x78, 0x40, 0x40, 0x40, 0x40);
    // G
    char_data!(39, 0x38, 0x44, 0x40, 0x5C, 0x44, 0x44, 0x38);
    // H
    char_data!(40, 0x44, 0x44, 0x7C, 0x44, 0x44, 0x44, 0x44);
    // I
    char_data!(41, 0x38, 0x10, 0x10, 0x10, 0x10, 0x10, 0x38);
    // J
    char_data!(42, 0x1C, 0x08, 0x08, 0x08, 0x08, 0x48, 0x30);
    // K
    char_data!(43, 0x44, 0x48, 0x50, 0x60, 0x50, 0x48, 0x44);
    // L
    char_data!(44, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7C);
    // M
    char_data!(45, 0x44, 0x6C, 0x54, 0x44, 0x44, 0x44, 0x44);
    // N
    char_data!(46, 0x44, 0x64, 0x54, 0x4C, 0x44, 0x44, 0x44);
    // O
    char_data!(47, 0x38, 0x44, 0x44, 0x44, 0x44, 0x44, 0x38);
    // P
    char_data!(48, 0x78, 0x44, 0x44, 0x78, 0x40, 0x40, 0x40);
    // Q
    char_data!(49, 0x38, 0x44, 0x44, 0x44, 0x54, 0x48, 0x34);
    // R
    char_data!(50, 0x78, 0x44, 0x44, 0x78, 0x50, 0x48, 0x44);
    // S
    char_data!(51, 0x38, 0x44, 0x40, 0x38, 0x04, 0x44, 0x38);
    // T
    char_data!(52, 0x7C, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10);
    // U
    char_data!(53, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x38);
    // V
    char_data!(54, 0x44, 0x44, 0x44, 0x28, 0x28, 0x10, 0x10);
    // W
    char_data!(55, 0x44, 0x44, 0x44, 0x44, 0x54, 0x6C, 0x44);
    // X
    char_data!(56, 0x44, 0x44, 0x28, 0x10, 0x28, 0x44, 0x44);
    // Y
    char_data!(57, 0x44, 0x44, 0x28, 0x10, 0x10, 0x10, 0x10);
    // Z
    char_data!(58, 0x7C, 0x04, 0x08, 0x10, 0x20, 0x40, 0x7C);

    // Digits 0-9 at indices 16-25 (0x30-0x39 mapped to 0x30-0x20=16..25)
    // 0
    char_data!(16, 0x38, 0x44, 0x4C, 0x54, 0x64, 0x44, 0x38);
    // 1
    char_data!(17, 0x10, 0x30, 0x10, 0x10, 0x10, 0x10, 0x38);
    // 2
    char_data!(18, 0x38, 0x44, 0x04, 0x08, 0x10, 0x20, 0x7C);
    // 3
    char_data!(19, 0x38, 0x44, 0x04, 0x18, 0x04, 0x44, 0x38);
    // 4
    char_data!(20, 0x08, 0x18, 0x28, 0x48, 0x7C, 0x08, 0x08);
    // 5
    char_data!(21, 0x7C, 0x40, 0x78, 0x04, 0x04, 0x44, 0x38);
    // 6
    char_data!(22, 0x38, 0x44, 0x40, 0x78, 0x44, 0x44, 0x38);
    // 7
    char_data!(23, 0x7C, 0x04, 0x08, 0x10, 0x20, 0x20, 0x20);
    // 8
    char_data!(24, 0x38, 0x44, 0x44, 0x38, 0x44, 0x44, 0x38);
    // 9
    char_data!(25, 0x38, 0x44, 0x44, 0x3C, 0x04, 0x44, 0x38);

    rom
};
