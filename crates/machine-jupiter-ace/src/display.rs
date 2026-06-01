//! Jupiter Ace character-based display.
//!
//! The Ace has a 32x24 character display. Each character position reads a
//! character code from video RAM ($2000-$23FF) and looks up the 8x8 pixel
//! pattern from character RAM ($2400-$27FF). The first 128 entries are
//! user-definable; codes 128-255 display the inverse of code 0-127.
//!
//! The display is monochrome: black on white (like the real hardware).
//!
//! # Timing
//!
//! The Ace uses a PAL display: 312 lines, 207 T-states per line = 64,584
//! T-states per frame at 3.25 MHz (~50.3 Hz). The active display area is
//! 256x192 pixels (32x24 characters x 8x8 pixels).

/// Framebuffer dimensions: 256x192 active area.
pub const FB_WIDTH: u32 = 256;
pub const FB_HEIGHT: u32 = 192;

/// Characters per row.
const CHARS_PER_ROW: usize = 32;
/// Character rows.
const CHAR_ROWS: usize = 24;
/// Pixels per character (width and height).
const CHAR_SIZE: usize = 8;

/// Video RAM size: 32x24 = 768 bytes.
pub const VIDEO_RAM_SIZE: usize = CHARS_PER_ROW * CHAR_ROWS;
/// Character RAM size: 128 characters x 8 bytes = 1024 bytes.
pub const CHAR_RAM_SIZE: usize = 1024;

/// T-states per scanline (PAL).
const TSTATES_PER_LINE: u32 = 207;
/// Total scanlines per frame (PAL).
const LINES_PER_FRAME: u32 = 312;
/// Total T-states per frame.
pub const TSTATES_PER_FRAME: u32 = TSTATES_PER_LINE * LINES_PER_FRAME;


/// Monochrome colours (ARGB32).
const WHITE: u32 = 0xFF_CF_CF_CF; // slightly warm white, like the real CRT
const BLACK: u32 = 0xFF_00_00_00;

/// Jupiter Ace display state.
///
/// Renders the character display to an ARGB32 framebuffer. The display reads
/// video RAM and character RAM from the bus each frame during `render_frame()`.
pub struct Display {
    /// ARGB32 framebuffer (256x192).
    framebuffer: Vec<u32>,
    /// T-state counter within the current frame.
    tstate_in_frame: u32,
    /// Whether the current frame is complete.
    frame_complete: bool,
    /// Speaker output state (bit 4 of port $FE).
    pub speaker_state: bool,
}

impl Display {
    #[must_use]
    pub fn new() -> Self {
        Self {
            framebuffer: vec![WHITE; (FB_WIDTH * FB_HEIGHT) as usize],
            tstate_in_frame: 0,
            frame_complete: false,
            speaker_state: false,
        }
    }

    /// Advance one T-state. Returns true when a frame boundary is crossed.
    pub fn tick(&mut self) {
        self.tstate_in_frame += 1;
        if self.tstate_in_frame >= TSTATES_PER_FRAME {
            self.tstate_in_frame = 0;
            self.frame_complete = true;
        }
    }

    /// Check and clear the frame-complete flag.
    pub fn take_frame_complete(&mut self) -> bool {
        let complete = self.frame_complete;
        self.frame_complete = false;
        complete
    }

    /// Whether the beam is in the active display area on the current T-state.
    /// Used for contention timing (not needed for the Ace, but kept for
    /// consistency with the framework).
    #[must_use]
    pub fn current_line(&self) -> u32 {
        self.tstate_in_frame / TSTATES_PER_LINE
    }

    /// Render the full character display from video RAM and character RAM.
    ///
    /// Called once per frame (just before signalling frame complete is typical).
    /// This is simpler than scanline-accurate rendering but sufficient for the
    /// Ace's character-based display, which has no mid-frame effects.
    pub fn render_frame(&mut self, video_ram: &[u8], char_ram: &[u8]) {
        for char_row in 0..CHAR_ROWS {
            for char_col in 0..CHARS_PER_ROW {
                let char_code = video_ram[char_row * CHARS_PER_ROW + char_col];
                let inverse = char_code & 0x80 != 0;
                let base_code = (char_code & 0x7F) as usize;

                for pixel_row in 0..CHAR_SIZE {
                    let pattern_byte = char_ram[base_code * CHAR_SIZE + pixel_row];
                    let fb_y = char_row * CHAR_SIZE + pixel_row;
                    let fb_x_base = char_col * CHAR_SIZE;

                    for pixel_col in 0..CHAR_SIZE {
                        let bit_set = pattern_byte & (0x80 >> pixel_col) != 0;
                        let pixel_on = if inverse { !bit_set } else { bit_set };
                        let colour = if pixel_on { BLACK } else { WHITE };
                        let fb_idx = fb_y * (FB_WIDTH as usize) + fb_x_base + pixel_col;
                        self.framebuffer[fb_idx] = colour;
                    }
                }
            }
        }
    }

    /// Reference to the ARGB32 framebuffer.
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

    /// Whether an interrupt should fire (top of frame, like the Spectrum).
    ///
    /// The Ace generates a maskable interrupt at the start of each frame
    /// (VSYNC period). This drives the keyboard scan and cursor flash.
    #[must_use]
    pub fn interrupt_active(&self) -> bool {
        // INT active during the first few T-states of the frame
        // (VSYNC period). The real hardware holds INT for ~32 T-states.
        self.tstate_in_frame < 32
    }

    /// T-state counter within the frame.
    #[must_use]
    pub fn tstate_in_frame(&self) -> u32 {
        self.tstate_in_frame
    }
}

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_correct_size() {
        let display = Display::new();
        assert_eq!(display.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
    }

    #[test]
    fn frame_complete_after_full_frame() {
        let mut display = Display::new();
        for _ in 0..TSTATES_PER_FRAME {
            display.tick();
        }
        assert!(display.take_frame_complete());
        assert!(!display.take_frame_complete()); // cleared after take
    }

    #[test]
    fn render_blank_screen() {
        let mut display = Display::new();
        let video_ram = vec![0u8; VIDEO_RAM_SIZE];
        let char_ram = vec![0u8; CHAR_RAM_SIZE];
        display.render_frame(&video_ram, &char_ram);
        // Character 0 with all-zero pattern = all white
        assert!(display.framebuffer().iter().all(|&p| p == WHITE));
    }

    #[test]
    fn render_solid_character() {
        let mut display = Display::new();
        let mut video_ram = vec![0u8; VIDEO_RAM_SIZE];
        let mut char_ram = vec![0u8; CHAR_RAM_SIZE];
        // Define character 1 as a solid block
        for row in 0..8 {
            char_ram[1 * 8 + row] = 0xFF;
        }
        // Place character 1 at position (0,0)
        video_ram[0] = 1;
        display.render_frame(&video_ram, &char_ram);
        // Top-left 8x8 should be black
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(display.framebuffer()[y * FB_WIDTH as usize + x], BLACK);
            }
        }
    }

    #[test]
    fn inverse_character() {
        let mut display = Display::new();
        let mut video_ram = vec![0u8; VIDEO_RAM_SIZE];
        let char_ram = vec![0u8; CHAR_RAM_SIZE]; // char 0 = all zero pattern
        // Place inverse of character 0 (code 0x80)
        video_ram[0] = 0x80;
        display.render_frame(&video_ram, &char_ram);
        // Inverse of all-zero pattern = all black
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(display.framebuffer()[y * FB_WIDTH as usize + x], BLACK);
            }
        }
    }
}
