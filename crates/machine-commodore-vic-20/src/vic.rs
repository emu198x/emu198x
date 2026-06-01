//! VIC 6560/6561 Video Interface Chip.
//!
//! The VIC chip handles both video AND audio for the VIC-20. Video output
//! is 22x23 characters in text mode (176x184 pixels). The chip also provides
//! 3 tone generators and 1 noise generator.
//!
//! For v1, text mode display is implemented. Audio is stubbed.

/// Framebuffer dimensions (visible area for text mode).
pub const ACTIVE_WIDTH: u32 = 176;
pub const ACTIVE_HEIGHT: u32 = 184;

/// Border thickness around the active area. The VIC chip generates a
/// substantial border around the active 22 x 23 character display;
/// VIC-20 reference emulators (VICE) typically render ~30-40 px of
/// border each side. 24 px L/R + 16 px T/B is a clean approximation
/// that matches the period look on a typical PAL television set.
pub const BORDER_LEFT: u32 = 24;
pub const BORDER_RIGHT: u32 = 24;
pub const BORDER_TOP: u32 = 16;
pub const BORDER_BOTTOM: u32 = 16;

pub const FB_WIDTH: u32 = ACTIVE_WIDTH + BORDER_LEFT + BORDER_RIGHT;
pub const FB_HEIGHT: u32 = ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM;

/// VIC 6560/6561 chip.
pub struct Vic6560 {
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// VIC registers ($9000-$900F).
    regs: [u8; 16],
    /// Whether a frame has completed.
    frame_complete: bool,
    /// Current scanline.
    scanline: u32,
    /// Current pixel position within the scanline.
    pixel_x: u32,
    /// Total lines per frame (PAL: 312, NTSC: 261).
    lines_per_frame: u32,
    /// Cycles per line (PAL: 71, NTSC: 65).
    cycles_per_line: u32,
}

impl Vic6560 {
    /// Create a new VIC chip.
    ///
    /// `pal`: true for PAL (6561), false for NTSC (6560).
    #[must_use]
    pub fn new(pal: bool) -> Self {
        let (lines, cycles) = if pal { (312, 71) } else { (261, 65) };
        Self {
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            regs: [0; 16],
            frame_complete: false,
            scanline: 0,
            pixel_x: 0,
            lines_per_frame: lines,
            cycles_per_line: cycles,
        }
    }

    /// Read a VIC register.
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        let reg = (addr & 0x0F) as usize;
        self.regs[reg]
    }

    /// Write a VIC register.
    pub fn write(&mut self, addr: u8, value: u8) {
        let reg = (addr & 0x0F) as usize;
        self.regs[reg] = value;
    }

    /// Tick one VIC cycle. Call with callbacks for reading screen RAM,
    /// colour RAM, and character ROM.
    pub fn tick(
        &mut self,
        read_screen: impl Fn(u16) -> u8,
        read_colour: impl Fn(u16) -> u8,
        read_char_rom: impl Fn(u16) -> u8,
    ) -> bool {
        self.pixel_x += 1;

        if self.pixel_x >= self.cycles_per_line {
            self.pixel_x = 0;
            self.scanline += 1;

            if self.scanline >= self.lines_per_frame {
                self.scanline = 0;
                self.frame_complete = true;
                return true;
            }
        }

        // Render during visible area
        // Text display: 22 columns x 23 rows = 176x184 pixels
        let visible_y_start = 28u32;
        let visible_y_end = visible_y_start + 184;

        // At the start of each new frame, repaint the entire framebuffer
        // with the VIC border colour (register $F low nibble) so the
        // border around the 176 x 184 active region carries the right
        // colour. Mid-frame border-colour changes affect the next frame
        // — v1 simplification.
        if self.scanline == 0 && self.pixel_x == 0 {
            let border_colour = VIC_PALETTE[(self.regs[0x0F] as usize) & 0x0F];
            self.framebuffer.fill(border_colour);
        }

        if self.scanline >= visible_y_start
            && self.scanline < visible_y_end
            && self.pixel_x < 22
        {
            let vis_y = self.scanline - visible_y_start;
            let char_row = vis_y / 8;
            let pixel_in_char_y = vis_y % 8;
            let char_col = self.pixel_x;

            // Screen memory base from registers
            let screen_base = ((u16::from(self.regs[5]) & 0xF0) as u16) << 6
                | ((u16::from(self.regs[2]) & 0x80) as u16) << 2;

            let char_addr = screen_base.wrapping_add(char_row as u16 * 22 + char_col as u16);
            let char_code = read_screen(char_addr);
            let colour_nibble = read_colour(char_addr) & 0x0F;

            // Character ROM lookup
            let char_rom_base = ((u16::from(self.regs[5]) & 0x0F) as u16) << 10;
            let char_rom_addr = char_rom_base.wrapping_add(u16::from(char_code) * 8 + pixel_in_char_y as u16);
            let char_data = read_char_rom(char_rom_addr);

            // Background colour from register $0F
            let bg_colour = VIC_PALETTE[(self.regs[0x0F] as usize >> 4) & 0x0F];
            let fg_colour = VIC_PALETTE[colour_nibble as usize];

            // Render 8 pixels for this character column, offset into
            // the active region of the framebuffer (skip the border).
            for px in 0..8 {
                let active_x = char_col * 8 + px;
                if active_x < ACTIVE_WIDTH {
                    let bit = (char_data >> (7 - px)) & 1;
                    let colour = if bit != 0 { fg_colour } else { bg_colour };
                    let fb_x = BORDER_LEFT + active_x;
                    let fb_y = BORDER_TOP + vis_y;
                    let idx = (fb_y * FB_WIDTH + fb_x) as usize;
                    if idx < self.framebuffer.len() {
                        self.framebuffer[idx] = colour;
                    }
                }
            }
        }

        false
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

    /// Current registers (for observation).
    #[must_use]
    pub fn regs(&self) -> &[u8; 16] {
        &self.regs
    }
}

impl Default for Vic6560 {
    fn default() -> Self {
        Self::new(true)
    }
}

/// VIC-20 colour palette (ARGB32).
static VIC_PALETTE: [u32; 16] = [
    0xFF00_0000, // 0  Black
    0xFFFF_FFFF, // 1  White
    0xFF78_2922, // 2  Red
    0xFF87_D6DD, // 3  Cyan
    0xFFAA_5FB6, // 4  Purple
    0xFF55_A049, // 5  Green
    0xFF40_31A2, // 6  Blue
    0xFFBF_CE72, // 7  Yellow
    0xFFAA_7449, // 8  Orange
    0xFFEA_B489, // 9  Light Orange
    0xFFB8_6962, // 10 Light Red
    0xFFC7_FF_FF, // 11 Light Cyan
    0xFFEA_9F_F6, // 12 Light Purple
    0xFF94_E0_89, // 13 Light Green
    0xFF87_71_F2, // 14 Light Blue
    0xFFFF_FF_B2, // 15 Light Yellow
];
