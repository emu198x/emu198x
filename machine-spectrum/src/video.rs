//! Video rendering for ZX Spectrum emulation.
//!
//! Handles the Spectrum's unique display format and renders to a framebuffer.

/// Native Spectrum screen width (256 pixels + 32 pixel border each side).
pub const NATIVE_WIDTH: u32 = 320;

/// Native Spectrum screen height (192 pixels + 32 pixel border top/bottom).
pub const NATIVE_HEIGHT: u32 = 256;

/// Border size in pixels.
const BORDER: usize = 32;

/// Spectrum color palette (ARGB format internally).
///
/// Index 0-7: normal colors, 8-15: bright colors.
/// Note: bright black is the same as normal black.
const COLOURS: [u32; 16] = [
    // Normal
    0xFF000000, // 0: black
    0xFF0000D7, // 1: blue
    0xFFD70000, // 2: red
    0xFFD700D7, // 3: magenta
    0xFF00D700, // 4: green
    0xFF00D7D7, // 5: cyan
    0xFFD7D700, // 6: yellow
    0xFFD7D7D7, // 7: white
    // Bright
    0xFF000000, // 8: black (same)
    0xFF0000FF, // 9: bright blue
    0xFFFF0000, // 10: bright red
    0xFFFF00FF, // 11: bright magenta
    0xFF00FF00, // 12: bright green
    0xFF00FFFF, // 13: bright cyan
    0xFFFFFF00, // 14: bright yellow
    0xFFFFFFFF, // 15: bright white
];

/// Convert ARGB u32 to RGBA bytes at a given index in the buffer.
#[inline]
fn write_rgba(buffer: &mut [u8], index: usize, color: u32) {
    buffer[index] = ((color >> 16) & 0xFF) as u8; // R
    buffer[index + 1] = ((color >> 8) & 0xFF) as u8; // G
    buffer[index + 2] = (color & 0xFF) as u8; // B
    buffer[index + 3] = 0xFF; // A
}

/// Render the Spectrum screen to a framebuffer.
///
/// # Arguments
/// * `screen` - The Spectrum's screen memory (6912 bytes: 6144 bitmap + 768 attributes)
/// * `border` - Current border color (0-7)
/// * `flash_swap` - Whether FLASH attribute should currently swap ink/paper
/// * `buffer` - Output framebuffer (NATIVE_WIDTH * NATIVE_HEIGHT * 4 bytes, RGBA format)
pub fn render_screen(screen: &[u8], border: u8, flash_swap: bool, buffer: &mut [u8]) {
    let border_colour = COLOURS[border as usize];

    for native_y in 0..(NATIVE_HEIGHT as usize) {
        for native_x in 0..(NATIVE_WIDTH as usize) {
            let pixel = if native_y < BORDER
                || native_y >= BORDER + 192
                || native_x < BORDER
                || native_x >= BORDER + 256
            {
                border_colour
            } else {
                let screen_y = native_y - BORDER;
                let screen_x = native_x - BORDER;
                render_pixel(screen, screen_x, screen_y, flash_swap)
            };

            // Write pixel to RGBA buffer (no scaling - pixels handles it)
            let pixel_index = (native_y * (NATIVE_WIDTH as usize) + native_x) * 4;
            write_rgba(buffer, pixel_index, pixel);
        }
    }
}

/// Calculate the bitmap address for a screen coordinate.
///
/// The Spectrum uses interleaved addressing where lines are not contiguous.
fn bitmap_address(screen_x: usize, screen_y: usize) -> usize {
    let x_byte = screen_x / 8;
    ((screen_y & 0xC0) << 5) | ((screen_y & 0x07) << 8) | ((screen_y & 0x38) << 2) | x_byte
}

/// Calculate the attribute address for a screen coordinate.
///
/// Attributes are stored at 0x1800 offset, 32 bytes per character row.
fn attribute_address(screen_x: usize, screen_y: usize) -> usize {
    let x_byte = screen_x / 8;
    0x1800 + (screen_y / 8) * 32 + x_byte
}

/// Render a single pixel from the Spectrum's screen memory.
fn render_pixel(screen: &[u8], screen_x: usize, screen_y: usize, flash_swap: bool) -> u32 {
    let bit = screen_x % 8;

    let byte = screen[bitmap_address(screen_x, screen_y)];
    let attr = screen[attribute_address(screen_x, screen_y)];

    let flash = attr & 0x80 != 0;
    let bright = if attr & 0x40 != 0 { 8 } else { 0 };
    let mut ink = (attr & 0x07) as usize + bright;
    let mut paper = ((attr >> 3) & 0x07) as usize + bright;

    // FLASH attribute swaps ink and paper every 16 frames
    if flash && flash_swap {
        std::mem::swap(&mut ink, &mut paper);
    }

    if byte & (0x80 >> bit) != 0 {
        COLOURS[ink]
    } else {
        COLOURS[paper]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_address_first_line() {
        assert_eq!(bitmap_address(0, 0), 0x0000);
        assert_eq!(bitmap_address(8, 0), 0x0001);
        assert_eq!(bitmap_address(248, 0), 0x001F);
    }

    #[test]
    fn bitmap_address_interleaved_lines() {
        assert_eq!(bitmap_address(0, 0), 0x0000);
        assert_eq!(bitmap_address(0, 1), 0x0100);
        assert_eq!(bitmap_address(0, 2), 0x0200);
        assert_eq!(bitmap_address(0, 7), 0x0700);
    }

    #[test]
    fn attribute_address_first_row() {
        assert_eq!(attribute_address(0, 0), 0x1800);
        assert_eq!(attribute_address(8, 0), 0x1801);
        assert_eq!(attribute_address(248, 0), 0x181F);
    }
}
