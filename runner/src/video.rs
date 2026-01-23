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

/// Render a single pixel from the Spectrum's screen memory.
fn render_pixel(screen: &[u8], screen_x: usize, screen_y: usize, flash_swap: bool) -> u32 {
    let x_byte = screen_x / 8;
    let bit = screen_x % 8;

    // Spectrum's interleaved bitmap address calculation
    let bitmap_addr =
        ((screen_y & 0xC0) << 5) | ((screen_y & 0x07) << 8) | ((screen_y & 0x38) << 2) | x_byte;

    // Attribute address (32 bytes per character row)
    let attr_addr = 0x1800 + (screen_y / 8) * 32 + x_byte;

    let byte = screen[bitmap_addr];
    let attr = screen[attr_addr];

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
