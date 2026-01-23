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

/// Spectrum color palette (ARGB format).
///
/// Index 0-7: normal colors, 8-15: bright colors.
/// Note: bright black is the same as normal black.
pub const COLOURS: [u32; 16] = [
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

/// Render the Spectrum screen to a framebuffer.
///
/// # Arguments
/// * `screen` - The Spectrum's screen memory (6912 bytes: 6144 bitmap + 768 attributes)
/// * `border` - Current border color (0-7)
/// * `flash_swap` - Whether FLASH attribute should currently swap ink/paper
/// * `buffer` - Output framebuffer (WIDTH * HEIGHT pixels)
pub fn render_screen(screen: &[u8], border: u8, flash_swap: bool, buffer: &mut [u32]) {
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

            // Write scaled pixel block
            for sy in 0..SCALE {
                for sx in 0..SCALE {
                    let dest_x = native_x * SCALE + sx;
                    let dest_y = native_y * SCALE + sy;
                    buffer[dest_y * WIDTH + dest_x] = pixel;
                }
            }
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

    // =========== Color Palette Tests ===========

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

    // =========== Render Tests ===========

    fn create_test_screen() -> Vec<u8> {
        // 6144 bytes bitmap + 768 bytes attributes = 6912 bytes
        vec![0u8; 6912]
    }

    #[test]
    fn render_border_top() {
        let screen = create_test_screen();
        let mut buffer = vec![0u32; WIDTH * HEIGHT];

        render_screen(&screen, 1, false, &mut buffer); // blue border

        // Top-left corner should be border color (blue)
        assert_eq!(buffer[0], COLOURS[1]);
    }

    #[test]
    fn render_border_color_index() {
        let screen = create_test_screen();
        let mut buffer = vec![0u32; WIDTH * HEIGHT];

        for border in 0..8 {
            render_screen(&screen, border, false, &mut buffer);
            assert_eq!(buffer[0], COLOURS[border as usize]);
        }
    }

    #[test]
    fn render_black_screen() {
        let screen = create_test_screen();
        let mut buffer = vec![0u32; WIDTH * HEIGHT];

        render_screen(&screen, 0, false, &mut buffer);

        // Center pixel should be black (paper color when bitmap is 0)
        let center_x = (BORDER + 128) * SCALE;
        let center_y = (BORDER + 96) * SCALE;
        assert_eq!(buffer[center_y * WIDTH + center_x], COLOURS[0]);
    }

    #[test]
    fn render_ink_pixel() {
        let mut screen = create_test_screen();

        // Set top-left bitmap byte to 0x80 (leftmost pixel set)
        screen[0] = 0x80;
        // Set attribute: ink=7 (white), paper=0 (black)
        screen[0x1800] = 0x07;

        let mut buffer = vec![0u32; WIDTH * HEIGHT];
        render_screen(&screen, 0, false, &mut buffer);

        // The pixel at screen position (0,0) is at buffer position (BORDER, BORDER) * SCALE
        let pixel_x = BORDER * SCALE;
        let pixel_y = BORDER * SCALE;
        assert_eq!(buffer[pixel_y * WIDTH + pixel_x], COLOURS[7]); // white ink
    }

    #[test]
    fn render_bright_attribute() {
        let mut screen = create_test_screen();

        screen[0] = 0x80; // leftmost pixel set
        // Attribute: bright=1, ink=1 (blue) -> bright blue (index 9)
        screen[0x1800] = 0x41;

        let mut buffer = vec![0u32; WIDTH * HEIGHT];
        render_screen(&screen, 0, false, &mut buffer);

        let pixel_x = BORDER * SCALE;
        let pixel_y = BORDER * SCALE;
        assert_eq!(buffer[pixel_y * WIDTH + pixel_x], COLOURS[9]); // bright blue
    }

    #[test]
    fn render_flash_no_swap() {
        let mut screen = create_test_screen();

        screen[0] = 0x80;
        // Attribute: flash=1, ink=7, paper=0
        screen[0x1800] = 0x87;

        let mut buffer = vec![0u32; WIDTH * HEIGHT];
        render_screen(&screen, 0, false, &mut buffer);

        let pixel_x = BORDER * SCALE;
        let pixel_y = BORDER * SCALE;
        assert_eq!(buffer[pixel_y * WIDTH + pixel_x], COLOURS[7]); // white (ink)
    }

    #[test]
    fn render_flash_with_swap() {
        let mut screen = create_test_screen();

        screen[0] = 0x80;
        // Attribute: flash=1, ink=7, paper=0
        screen[0x1800] = 0x87;

        let mut buffer = vec![0u32; WIDTH * HEIGHT];
        render_screen(&screen, 0, true, &mut buffer); // flash_swap = true

        let pixel_x = BORDER * SCALE;
        let pixel_y = BORDER * SCALE;
        // With flash swap, ink and paper are swapped, so set pixel shows paper color
        assert_eq!(buffer[pixel_y * WIDTH + pixel_x], COLOURS[0]); // black (was paper)
    }

    #[test]
    fn render_scaling() {
        let screen = create_test_screen();
        let mut buffer = vec![0u32; WIDTH * HEIGHT];

        render_screen(&screen, 2, false, &mut buffer); // red border

        // Each native pixel should be a SCALE x SCALE block
        // Check that top-left SCALE x SCALE pixels are all the same
        let expected = COLOURS[2];
        for y in 0..SCALE {
            for x in 0..SCALE {
                assert_eq!(buffer[y * WIDTH + x], expected);
            }
        }
    }

    #[test]
    fn constants_are_consistent() {
        assert_eq!(WIDTH, NATIVE_WIDTH * SCALE);
        assert_eq!(HEIGHT, NATIVE_HEIGHT * SCALE);
        assert_eq!(NATIVE_WIDTH, 256 + BORDER * 2);
        assert_eq!(NATIVE_HEIGHT, 192 + BORDER * 2);
    }
}
