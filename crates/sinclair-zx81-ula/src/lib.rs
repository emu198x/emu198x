//! ZX81 ULA (Uncommitted Logic Array).
//!
//! The ZX81 ULA handles display generation, NMI timing, and keyboard scanning.
//! Unlike the ZX Spectrum ULA, the ZX81 generates its display by stealing the
//! Z80 bus during HALT instructions — the NMI handler at $0066 executes a HALT,
//! and the ULA responds by putting character ROM data on the data bus during
//! the Z80's refresh cycles.
//!
//! # Display
//!
//! 32 columns x 24 rows of 8x8 pixel characters (256x192 active pixels).
//! The display file (D_FILE) is at a variable address stored in system variable
//! at $400C-$400D. Each row is terminated by a $76 (HALT opcode / NEWLINE).
//! Characters 0-63 are normal, 128-191 are inverse video.
//!
//! # Timing (PAL)
//!
//! - Z80 clock: 3.25 MHz
//! - Lines per frame: 312
//! - T-states per line: 207 (based on NMI period of ~64µs)
//! - Total T-states per frame: ~64,584
//! - Frame rate: ~50.3 Hz
//!
//! # Simplified v1 Model
//!
//! This implementation counts scanlines, generates NMI at the right time,
//! and renders the display file from RAM to a framebuffer. It does not
//! model the bus-stealing display mechanism or the character ROM overlay
//! on the data bus.

/// Framebuffer dimensions.
///
/// 320x240 gives a 4:3 display with 32-pixel borders around the 256x192
/// active area.
pub const FB_WIDTH: u32 = 320;
pub const FB_HEIGHT: u32 = 240;

/// Active display area within the framebuffer.
const BORDER_LEFT: u32 = 32;
const BORDER_TOP: u32 = 24;
const SCREEN_HEIGHT: u32 = 192;

/// T-states per scanline.
const TSTATES_PER_LINE: u32 = 207;

/// Total scanlines per frame (PAL).
const LINES_PER_FRAME: u32 = 312;

/// First scanline of the active display area.
/// VSync occupies the first ~56 lines, then top border, then screen starts.
const FIRST_SCREEN_LINE: u32 = 56;

/// T-states per frame.
const TSTATES_PER_FRAME: u32 = TSTATES_PER_LINE * LINES_PER_FRAME;

/// Characters per row in the display.
const CHARS_PER_ROW: usize = 32;

/// Character rows in the display.
const CHAR_ROWS: usize = 24;

/// HALT opcode / NEWLINE character in the display file.
const NEWLINE: u8 = 0x76;

/// Black pixel (ARGB32).
const BLACK: u32 = 0xFF00_0000;

/// White pixel (ARGB32).
const WHITE: u32 = 0xFFFF_FFFF;

/// ZX81 palette: black and white only.
pub const PALETTE: [u32; 2] = [WHITE, BLACK];

/// ZX81 ULA state.
pub struct Zx81Ula {
    /// Current T-state within the frame (0 .. TSTATES_PER_FRAME-1).
    tstate: u32,
    /// Whether a frame has completed (auto-clears on read).
    frame_complete: bool,
    /// Whether NMI should be asserted this tick.
    nmi_active: bool,
    /// Current scanline (derived from tstate).
    line: u32,
    /// Inverse video mode (set by system variable MARGIN at $4028).
    inverse_video: bool,
    /// ARGB32 framebuffer.
    framebuffer: Vec<u32>,
    /// Frame counter.
    frame_count: u64,
}

impl Zx81Ula {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tstate: 0,
            frame_complete: false,
            nmi_active: false,
            inverse_video: false,
            line: 0,
            framebuffer: vec![WHITE; (FB_WIDTH * FB_HEIGHT) as usize],
            frame_count: 0,
        }
    }

    /// Advance the ULA by one CPU T-state (3.25 MHz).
    ///
    /// The `read_mem` closure reads a byte from the system's RAM/ROM without
    /// side effects.
    pub fn tick(&mut self, read_mem: impl Fn(u16) -> u8) {
        self.tstate += 1;
        self.line = self.tstate / TSTATES_PER_LINE;

        // NMI is generated at the start of each display line during the active area.
        // The NMI drives the display generation — each NMI triggers the NMI handler
        // which executes HALT, and the ULA renders one line of characters.
        let in_display = self.line >= FIRST_SCREEN_LINE
            && self.line < FIRST_SCREEN_LINE + SCREEN_HEIGHT;
        let line_tstate = self.tstate % TSTATES_PER_LINE;

        // Assert NMI at the start of each active display line
        self.nmi_active = in_display && line_tstate == 0;

        if self.tstate >= TSTATES_PER_FRAME {
            self.tstate = 0;
            self.line = 0;
            self.frame_complete = true;
            self.frame_count += 1;

            // Render the display file to the framebuffer at frame boundaries.
            // This is a simplified approach — a real ZX81 renders line-by-line
            // during the NMI/HALT bus-stealing cycle.
            self.render_display(&read_mem);
        }
    }

    /// Whether the NMI line should be asserted this tick.
    #[must_use]
    pub fn nmi_active(&self) -> bool {
        self.nmi_active
    }

    /// Has the frame completed? Auto-clears on read.
    pub fn take_frame_complete(&mut self) -> bool {
        let result = self.frame_complete;
        self.frame_complete = false;
        result
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

    /// Total T-states per frame.
    #[must_use]
    pub fn tstates_per_frame(&self) -> u32 {
        TSTATES_PER_FRAME
    }

    /// Current scanline.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// Current T-state within the current line.
    #[must_use]
    pub fn line_tstate(&self) -> u32 {
        self.tstate % TSTATES_PER_LINE
    }

    /// Current frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Set the inverse video flag.
    pub fn set_inverse(&mut self, inverse: bool) {
        self.inverse_video = inverse;
    }

    /// Read the keyboard matrix.
    ///
    /// The ZX81 keyboard is an 8x5 matrix identical to the Spectrum layout.
    /// `addr_high` selects rows via active-low address lines A8-A15.
    /// `rows` is the 8-byte keyboard state (1 = pressed, inverted on output).
    ///
    /// Returns bits 0-4 (active low: 0 = pressed), bits 5-7 = 1.
    #[must_use]
    pub fn read_keyboard(addr_high: u8, rows: &[u8; 8]) -> u8 {
        let mut result: u8 = 0;
        for (i, &row) in rows.iter().enumerate() {
            if addr_high & (1 << i) == 0 {
                result |= row;
            }
        }
        (!result & 0x1F) | 0xE0
    }

    /// Render the display file to the framebuffer.
    ///
    /// The display file is at the address stored in D_FILE ($400C-$400D).
    /// Each character row is a sequence of character codes terminated by $76.
    /// Character ROM data is in the first 512 bytes of the system ROM.
    fn render_display(&mut self, read_mem: &impl Fn(u16) -> u8) {
        // Clear framebuffer to white (border colour)
        self.framebuffer.fill(WHITE);

        // Read D_FILE pointer from system variables
        let d_file_lo = read_mem(0x400C);
        let d_file_hi = read_mem(0x400D);
        let mut d_file = u16::from(d_file_lo) | (u16::from(d_file_hi) << 8);

        // Skip the initial HALT/NEWLINE byte
        let first = read_mem(d_file);
        if first == NEWLINE {
            d_file = d_file.wrapping_add(1);
        }

        for row in 0..CHAR_ROWS {
            let fb_y_base = BORDER_TOP + (row as u32) * 8;

            for col in 0..CHARS_PER_ROW {
                let ch = read_mem(d_file);
                d_file = d_file.wrapping_add(1);

                if ch == NEWLINE {
                    // End of row — remaining columns are blank (spaces)
                    // Fill remaining columns with white
                    for fill_col in col..CHARS_PER_ROW {
                        let fb_x_base = BORDER_LEFT + (fill_col as u32) * 8;
                        for py in 0..8u32 {
                            let fb_y = fb_y_base + py;
                            if fb_y >= FB_HEIGHT {
                                break;
                            }
                            for px in 0..8u32 {
                                let fb_x = fb_x_base + px;
                                if fb_x < FB_WIDTH {
                                    self.framebuffer[(fb_y * FB_WIDTH + fb_x) as usize] = WHITE;
                                }
                            }
                        }
                    }
                    break;
                }

                // Character code: bits 0-5 = character index (0-63),
                // bit 7 = inverse flag
                let char_index = (ch & 0x3F) as u16;
                let inverse = ch & 0x80 != 0;

                let fb_x_base = BORDER_LEFT + (col as u32) * 8;

                // Read 8 bytes from character ROM (first 512 bytes of ROM)
                for py in 0..8u32 {
                    let fb_y = fb_y_base + py;
                    if fb_y >= FB_HEIGHT {
                        break;
                    }

                    let rom_addr = char_index * 8 + py as u16;
                    let mut pattern = read_mem(rom_addr);

                    if inverse {
                        pattern = !pattern;
                    }

                    for px in 0..8u32 {
                        let fb_x = fb_x_base + px;
                        if fb_x >= FB_WIDTH {
                            continue;
                        }

                        let bit = 7 - px;
                        let pixel = if pattern & (1 << bit) != 0 {
                            BLACK
                        } else {
                            WHITE
                        };
                        self.framebuffer[(fb_y * FB_WIDTH + fb_x) as usize] = pixel;
                    }
                }
            }

            // Skip the NEWLINE terminator if we consumed all 32 characters
            // without hitting a NEWLINE
            let next = read_mem(d_file);
            if next == NEWLINE {
                d_file = d_file.wrapping_add(1);
            }
        }
    }
}

impl Default for Zx81Ula {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing() {
        assert_eq!(TSTATES_PER_FRAME, 207 * 312);
        assert_eq!(TSTATES_PER_FRAME, 64_584);
    }

    #[test]
    fn framebuffer_size() {
        let ula = Zx81Ula::new();
        assert_eq!(ula.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
        assert_eq!(ula.framebuffer_width(), 320);
        assert_eq!(ula.framebuffer_height(), 240);
    }

    #[test]
    fn frame_complete_auto_clears() {
        let mut ula = Zx81Ula::new();
        let zeros = [0u8; 0x10000];
        for _ in 0..TSTATES_PER_FRAME {
            ula.tick(|addr| zeros[addr as usize]);
        }
        assert!(ula.take_frame_complete());
        assert!(!ula.take_frame_complete());
    }

    #[test]
    fn keyboard_no_keys() {
        let rows = [0u8; 8];
        assert_eq!(Zx81Ula::read_keyboard(0x00, &rows), 0xFF);
    }

    #[test]
    fn keyboard_single_key() {
        let mut rows = [0u8; 8];
        rows[0] = 0x01; // Shift pressed
        // Scan row 0 (A8 = 0 → addr_high bit 0 clear)
        assert_eq!(Zx81Ula::read_keyboard(0xFE, &rows) & 0x1F, 0x1E);
        // Scan row 1 (A9 = 0) — shift not visible
        assert_eq!(Zx81Ula::read_keyboard(0xFD, &rows), 0xFF);
    }

    #[test]
    fn nmi_during_display() {
        let mut ula = Zx81Ula::new();
        let zeros = [0u8; 0x10000];

        // Tick to the start of the first display line
        let target = FIRST_SCREEN_LINE * TSTATES_PER_LINE;
        for _ in 0..target {
            ula.tick(|addr| zeros[addr as usize]);
        }
        // The tick that crossed the line boundary should have set NMI
        assert!(ula.nmi_active());
    }
}
