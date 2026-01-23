//! Video rendering for ZX Spectrum emulation.
//!
//! Handles the Spectrum's unique display format and renders to a framebuffer.
//! Supports mid-scanline border color changes for accurate demo effects.

/// Native Spectrum screen width (256 pixels + 32 pixel border each side).
pub const NATIVE_WIDTH: u32 = 320;

/// Native Spectrum screen height (192 pixels + 32 pixel border top/bottom).
pub const NATIVE_HEIGHT: u32 = 256;

/// Border size in pixels.
const BORDER: usize = 32;

/// T-states per scanline.
const T_STATES_PER_LINE: u32 = 224;

/// First visible scanline (after vertical sync/blanking).
/// The 48K Spectrum has 64 lines of top border before the display area.
const FIRST_VISIBLE_LINE: u32 = 16;

/// T-state offset within a line where the left border starts being visible.
/// This accounts for horizontal sync/blanking at the start of each line.
const LINE_VISIBLE_START: u32 = 24;

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

/// Convert screen pixel coordinates to T-state within a frame.
///
/// The visible area maps to the ULA's timing:
/// - 312 scanlines total, 224 T-states per line
/// - Our 256-line display shows lines 16-271 of the frame (approximately)
/// - Each pixel is approximately 0.5 T-states (2 pixels per T-state)
#[inline]
fn pixel_to_t_state(native_x: usize, native_y: usize) -> u32 {
    // Map our visible 256 lines to frame scanlines
    // Line 0 of our display corresponds to frame line FIRST_VISIBLE_LINE
    let scanline = FIRST_VISIBLE_LINE + native_y as u32;

    // Each line has 224 T-states total
    // Our 320 pixels map to approximately 160 T-states of visible area
    // Starting at LINE_VISIBLE_START T-states into the line
    let line_t_state = LINE_VISIBLE_START + (native_x as u32) / 2;

    scanline * T_STATES_PER_LINE + line_t_state
}

/// Look up the border color at a given T-state.
///
/// Uses binary search to find the last transition before or at the given T-state.
#[inline]
fn border_color_at(transitions: &[(u32, u8)], t_state: u32) -> u8 {
    // Binary search for the insertion point
    match transitions.binary_search_by_key(&t_state, |&(t, _)| t) {
        Ok(idx) => transitions[idx].1,
        Err(idx) => {
            if idx == 0 {
                // Before first transition - use first color (shouldn't happen)
                transitions.first().map(|&(_, c)| c).unwrap_or(7)
            } else {
                // Use the color from the previous transition
                transitions[idx - 1].1
            }
        }
    }
}

/// Render the Spectrum screen to a framebuffer.
///
/// # Arguments
/// * `screen` - The Spectrum's screen memory (6912 bytes: 6144 bitmap + 768 attributes)
/// * `border_transitions` - List of (t_state, color) pairs for mid-scanline border effects
/// * `snow_events` - List of (display_line, char_column) pairs where snow occurred
/// * `flash_swap` - Whether FLASH attribute should currently swap ink/paper
/// * `buffer` - Output framebuffer (NATIVE_WIDTH * NATIVE_HEIGHT * 4 bytes, RGBA format)
pub fn render_screen(
    screen: &[u8],
    border_transitions: &[(u32, u8)],
    snow_events: &[(u32, u32)],
    flash_swap: bool,
    buffer: &mut [u8],
) {
    for native_y in 0..(NATIVE_HEIGHT as usize) {
        for native_x in 0..(NATIVE_WIDTH as usize) {
            let pixel = if native_y < BORDER
                || native_y >= BORDER + 192
                || native_x < BORDER
                || native_x >= BORDER + 256
            {
                // Border area - look up color based on T-state
                let t_state = pixel_to_t_state(native_x, native_y);
                let border_color = border_color_at(border_transitions, t_state);
                COLOURS[border_color as usize]
            } else {
                let screen_y = native_y - BORDER;
                let screen_x = native_x - BORDER;

                // Check if this character cell has snow
                let char_column = screen_x / 8;
                let has_snow = snow_events
                    .iter()
                    .any(|&(line, col)| line as usize == screen_y && col as usize == char_column);

                if has_snow {
                    render_snow_pixel(screen, screen_x, screen_y, flash_swap)
                } else {
                    render_pixel(screen, screen_x, screen_y, flash_swap)
                }
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

/// Render a single pixel with snow effect corruption.
///
/// When snow occurs, the ULA reads corrupted data due to bus conflict with the CPU.
/// The visual effect is that the bitmap data gets mixed with the attribute data,
/// creating a scrambled pattern. This simulates the real hardware behavior where
/// the ULA reads garbage when the CPU accesses screen memory simultaneously.
fn render_snow_pixel(screen: &[u8], screen_x: usize, screen_y: usize, flash_swap: bool) -> u32 {
    let bit = screen_x % 8;

    let byte = screen[bitmap_address(screen_x, screen_y)];
    let attr = screen[attribute_address(screen_x, screen_y)];

    // Snow corruption: XOR bitmap with attribute to simulate bus conflict
    // This creates the characteristic "snow" visual pattern
    let corrupted_byte = byte ^ attr;

    let flash = attr & 0x80 != 0;
    let bright = if attr & 0x40 != 0 { 8 } else { 0 };
    let mut ink = (attr & 0x07) as usize + bright;
    let mut paper = ((attr >> 3) & 0x07) as usize + bright;

    if flash && flash_swap {
        std::mem::swap(&mut ink, &mut paper);
    }

    // Use the corrupted bitmap data
    if corrupted_byte & (0x80 >> bit) != 0 {
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

    #[test]
    fn border_color_at_single_color() {
        // Single color for entire frame
        let transitions = vec![(0, 2)]; // Red border
        assert_eq!(border_color_at(&transitions, 0), 2);
        assert_eq!(border_color_at(&transitions, 10000), 2);
        assert_eq!(border_color_at(&transitions, 69887), 2);
    }

    #[test]
    fn border_color_at_mid_frame_change() {
        // Border changes mid-frame
        let transitions = vec![(0, 1), (35000, 4)]; // Blue, then green
        assert_eq!(border_color_at(&transitions, 0), 1);
        assert_eq!(border_color_at(&transitions, 34999), 1);
        assert_eq!(border_color_at(&transitions, 35000), 4);
        assert_eq!(border_color_at(&transitions, 69887), 4);
    }

    #[test]
    fn border_color_at_multiple_changes() {
        // Rapid border color changes (demo effect)
        let transitions = vec![
            (0, 0),     // Black
            (10000, 2), // Red
            (20000, 4), // Green
            (30000, 6), // Yellow
        ];
        assert_eq!(border_color_at(&transitions, 5000), 0);
        assert_eq!(border_color_at(&transitions, 15000), 2);
        assert_eq!(border_color_at(&transitions, 25000), 4);
        assert_eq!(border_color_at(&transitions, 35000), 6);
    }

    #[test]
    fn pixel_to_t_state_increases_left_to_right() {
        // T-state should increase as we move right across the screen
        let t1 = pixel_to_t_state(0, 0);
        let t2 = pixel_to_t_state(100, 0);
        let t3 = pixel_to_t_state(319, 0);
        assert!(t1 < t2);
        assert!(t2 < t3);
    }

    #[test]
    fn pixel_to_t_state_increases_top_to_bottom() {
        // T-state should increase as we move down the screen
        let t1 = pixel_to_t_state(0, 0);
        let t2 = pixel_to_t_state(0, 100);
        let t3 = pixel_to_t_state(0, 255);
        assert!(t1 < t2);
        assert!(t2 < t3);
    }
}
