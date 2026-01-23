//! Video rendering for ZX Spectrum emulation.
//!
//! Handles the Spectrum's unique display format and renders to a framebuffer.

/// Native Spectrum screen width (256 pixels + 32 pixel border each side).
pub const NATIVE_WIDTH: usize = 320;

/// Native Spectrum screen height (192 pixels + 32 pixel border top/bottom).
pub const NATIVE_HEIGHT: usize = 256;

/// Border size in pixels.
pub const BORDER: usize = 32;

/// Integer scale factor for sharp pixels.
pub const SCALE: usize = 3;

/// Scaled window width.
pub const WIDTH: usize = NATIVE_WIDTH * SCALE;

/// Scaled window height.
pub const HEIGHT: usize = NATIVE_HEIGHT * SCALE;

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

    for native_y in 0..NATIVE_HEIGHT {
        for native_x in 0..NATIVE_WIDTH {
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
            let pixel_index = (native_y * NATIVE_WIDTH + native_x) * 4;
            write_rgba(buffer, pixel_index, pixel);
        }
    }
}

/// Calculate the bitmap address for a screen coordinate.
///
/// The Spectrum uses interleaved addressing where lines are not contiguous.
pub fn bitmap_address(screen_x: usize, screen_y: usize) -> usize {
    let x_byte = screen_x / 8;
    ((screen_y & 0xC0) << 5) | ((screen_y & 0x07) << 8) | ((screen_y & 0x38) << 2) | x_byte
}

/// Calculate the attribute address for a screen coordinate.
///
/// Attributes are stored at 0x1800 offset, 32 bytes per character row.
pub fn attribute_address(screen_x: usize, screen_y: usize) -> usize {
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

    // =========== Bitmap Address Tests ===========

    #[test]
    fn bitmap_address_first_line() {
        // Line 0: addresses 0x0000-0x001F
        assert_eq!(bitmap_address(0, 0), 0x0000);
        assert_eq!(bitmap_address(8, 0), 0x0001);
        assert_eq!(bitmap_address(248, 0), 0x001F);
    }

    #[test]
    fn bitmap_address_interleaved_lines() {
        // Lines within first character row are interleaved
        // Line 0 -> 0x0000, Line 1 -> 0x0100, Line 2 -> 0x0200, etc.
        assert_eq!(bitmap_address(0, 0), 0x0000);
        assert_eq!(bitmap_address(0, 1), 0x0100);
        assert_eq!(bitmap_address(0, 2), 0x0200);
        assert_eq!(bitmap_address(0, 7), 0x0700);
    }

    #[test]
    fn bitmap_address_second_char_row() {
        // Line 8 starts at 0x0020 (second character row)
        assert_eq!(bitmap_address(0, 8), 0x0020);
        assert_eq!(bitmap_address(0, 9), 0x0120);
    }

    #[test]
    fn bitmap_address_second_third() {
        // Line 64 (second third of screen) starts at 0x0800
        assert_eq!(bitmap_address(0, 64), 0x0800);
    }

    #[test]
    fn bitmap_address_last_third() {
        // Line 128 (last third of screen) starts at 0x1000
        assert_eq!(bitmap_address(0, 128), 0x1000);
    }

    #[test]
    fn bitmap_address_last_line() {
        // Line 191 is the last line
        assert_eq!(bitmap_address(0, 191), 0x17E0);
        assert_eq!(bitmap_address(248, 191), 0x17FF);
    }

    // =========== Attribute Address Tests ===========

    #[test]
    fn attribute_address_first_row() {
        // First character row: 0x1800-0x181F
        assert_eq!(attribute_address(0, 0), 0x1800);
        assert_eq!(attribute_address(8, 0), 0x1801);
        assert_eq!(attribute_address(248, 0), 0x181F);
    }

    #[test]
    fn attribute_address_same_for_char_row() {
        // All 8 pixel lines in a character row share the same attribute
        for y in 0..8 {
            assert_eq!(attribute_address(0, y), 0x1800);
            assert_eq!(attribute_address(8, y), 0x1801);
        }
    }

    #[test]
    fn attribute_address_second_row() {
        assert_eq!(attribute_address(0, 8), 0x1820);
        assert_eq!(attribute_address(0, 15), 0x1820);
    }

    #[test]
    fn attribute_address_last_row() {
        // Last attribute row: 0x1AE0-0x1AFF
        assert_eq!(attribute_address(0, 184), 0x1AE0);
        assert_eq!(attribute_address(248, 191), 0x1AFF);
    }

    // =========== Color Tests ===========

    #[test]
    fn palette_normal_colors() {
        assert_eq!(COLOURS[0], 0xFF000000); // black
        assert_eq!(COLOURS[1], 0xFF0000D7); // blue
        assert_eq!(COLOURS[2], 0xFFD70000); // red
        assert_eq!(COLOURS[7], 0xFFD7D7D7); // white
    }

    #[test]
    fn palette_bright_colors() {
        assert_eq!(COLOURS[8], 0xFF000000); // bright black = black
        assert_eq!(COLOURS[9], 0xFF0000FF); // bright blue
        assert_eq!(COLOURS[15], 0xFFFFFFFF); // bright white
    }

    #[test]
    fn palette_has_alpha_channel() {
        // All colors should have full alpha
        for color in COLOURS.iter() {
            assert_eq!(color >> 24, 0xFF);
        }
    }

    // =========== RGBA Conversion Tests ===========

    #[test]
    fn write_rgba_black() {
        let mut buffer = [0u8; 4];
        write_rgba(&mut buffer, 0, COLOURS[0]); // black
        assert_eq!(buffer, [0x00, 0x00, 0x00, 0xFF]);
    }

    #[test]
    fn write_rgba_white() {
        let mut buffer = [0u8; 4];
        write_rgba(&mut buffer, 0, COLOURS[15]); // bright white
        assert_eq!(buffer, [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn write_rgba_red() {
        let mut buffer = [0u8; 4];
        write_rgba(&mut buffer, 0, COLOURS[2]); // red (0xFFD70000 ARGB)
        assert_eq!(buffer, [0xD7, 0x00, 0x00, 0xFF]); // RGBA
    }

    #[test]
    fn write_rgba_blue() {
        let mut buffer = [0u8; 4];
        write_rgba(&mut buffer, 0, COLOURS[1]); // blue (0xFF0000D7 ARGB)
        assert_eq!(buffer, [0x00, 0x00, 0xD7, 0xFF]); // RGBA
    }

    // =========== Render Tests ===========

    fn create_test_screen() -> Vec<u8> {
        // 6144 bytes bitmap + 768 bytes attributes = 6912 bytes
        vec![0u8; 6912]
    }

    #[test]
    fn render_border_top() {
        let screen = create_test_screen();
        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];

        render_screen(&screen, 1, false, &mut buffer); // blue border

        // Top-left corner should be border color (blue = 0xFF0000D7 ARGB -> [0x00, 0x00, 0xD7, 0xFF] RGBA)
        assert_eq!(&buffer[0..4], &[0x00, 0x00, 0xD7, 0xFF]);
    }

    #[test]
    fn render_border_color_index() {
        let screen = create_test_screen();
        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];

        for border in 0..8u8 {
            render_screen(&screen, border, false, &mut buffer);
            let expected_color = COLOURS[border as usize];
            let r = ((expected_color >> 16) & 0xFF) as u8;
            let g = ((expected_color >> 8) & 0xFF) as u8;
            let b = (expected_color & 0xFF) as u8;
            assert_eq!(&buffer[0..4], &[r, g, b, 0xFF]);
        }
    }

    #[test]
    fn render_black_screen() {
        let screen = create_test_screen();
        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];

        render_screen(&screen, 0, false, &mut buffer);

        // Center pixel should be black (paper color when bitmap is 0)
        let center_x = BORDER + 128;
        let center_y = BORDER + 96;
        let pixel_index = (center_y * NATIVE_WIDTH + center_x) * 4;
        assert_eq!(
            &buffer[pixel_index..pixel_index + 4],
            &[0x00, 0x00, 0x00, 0xFF]
        );
    }

    #[test]
    fn render_ink_pixel() {
        let mut screen = create_test_screen();

        // Set top-left bitmap byte to 0x80 (leftmost pixel set)
        screen[0] = 0x80;
        // Set attribute: ink=7 (white), paper=0 (black)
        screen[0x1800] = 0x07;

        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];
        render_screen(&screen, 0, false, &mut buffer);

        // The pixel at screen position (0,0) is at buffer position (BORDER, BORDER)
        let pixel_x = BORDER;
        let pixel_y = BORDER;
        let pixel_index = (pixel_y * NATIVE_WIDTH + pixel_x) * 4;
        // white = 0xFFD7D7D7 ARGB -> [0xD7, 0xD7, 0xD7, 0xFF] RGBA
        assert_eq!(
            &buffer[pixel_index..pixel_index + 4],
            &[0xD7, 0xD7, 0xD7, 0xFF]
        );
    }

    #[test]
    fn render_bright_attribute() {
        let mut screen = create_test_screen();

        screen[0] = 0x80; // leftmost pixel set
        // Attribute: bright=1, ink=1 (blue) -> bright blue (index 9)
        screen[0x1800] = 0x41;

        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];
        render_screen(&screen, 0, false, &mut buffer);

        let pixel_x = BORDER;
        let pixel_y = BORDER;
        let pixel_index = (pixel_y * NATIVE_WIDTH + pixel_x) * 4;
        // bright blue = 0xFF0000FF ARGB -> [0x00, 0x00, 0xFF, 0xFF] RGBA
        assert_eq!(
            &buffer[pixel_index..pixel_index + 4],
            &[0x00, 0x00, 0xFF, 0xFF]
        );
    }

    #[test]
    fn render_flash_no_swap() {
        let mut screen = create_test_screen();

        screen[0] = 0x80;
        // Attribute: flash=1, ink=7, paper=0
        screen[0x1800] = 0x87;

        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];
        render_screen(&screen, 0, false, &mut buffer);

        let pixel_x = BORDER;
        let pixel_y = BORDER;
        let pixel_index = (pixel_y * NATIVE_WIDTH + pixel_x) * 4;
        // white ink
        assert_eq!(
            &buffer[pixel_index..pixel_index + 4],
            &[0xD7, 0xD7, 0xD7, 0xFF]
        );
    }

    #[test]
    fn render_flash_with_swap() {
        let mut screen = create_test_screen();

        screen[0] = 0x80;
        // Attribute: flash=1, ink=7, paper=0
        screen[0x1800] = 0x87;

        let mut buffer = vec![0u8; NATIVE_WIDTH * NATIVE_HEIGHT * 4];
        render_screen(&screen, 0, true, &mut buffer); // flash_swap = true

        let pixel_x = BORDER;
        let pixel_y = BORDER;
        let pixel_index = (pixel_y * NATIVE_WIDTH + pixel_x) * 4;
        // With flash swap, ink and paper are swapped, so set pixel shows paper color (black)
        assert_eq!(
            &buffer[pixel_index..pixel_index + 4],
            &[0x00, 0x00, 0x00, 0xFF]
        );
    }

    #[test]
    fn constants_are_consistent() {
        assert_eq!(WIDTH, NATIVE_WIDTH * SCALE);
        assert_eq!(HEIGHT, NATIVE_HEIGHT * SCALE);
        assert_eq!(NATIVE_WIDTH, 256 + BORDER * 2);
        assert_eq!(NATIVE_HEIGHT, 192 + BORDER * 2);
    }
}
