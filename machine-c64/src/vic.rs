//! VIC-II video chip emulation.
//!
//! The VIC-II is responsible for generating the C64's video output.
//! This implementation supports:
//! - Standard text mode (40x25 characters)
//! - Multicolor text mode
//! - Standard bitmap mode
//! - Multicolor bitmap mode
//! - Sprites (future)
//! - Raster interrupts

use crate::memory::Memory;

/// Display width including borders.
pub const DISPLAY_WIDTH: u32 = 384;

/// Display height including borders.
pub const DISPLAY_HEIGHT: u32 = 272;

/// Border size on left/right.
const BORDER_H: usize = 32;

/// Border size on top/bottom.
const BORDER_V: usize = 36;

/// VIC-II video chip.
pub struct Vic {
    /// Current raster line
    pub raster_line: u16,
    /// Raster interrupt line
    pub raster_irq: u16,
}

impl Vic {
    pub fn new() -> Self {
        Self {
            raster_line: 0,
            raster_irq: 0,
        }
    }

    /// Check if a VIC interrupt should fire.
    pub fn check_irq(&self, memory: &Memory) -> bool {
        // Check raster IRQ
        if self.raster_line == self.raster_irq {
            let irq_enable = memory.vic_registers[0x1A];
            if irq_enable & 0x01 != 0 {
                return true;
            }
        }
        false
    }

    /// Reset the VIC-II.
    pub fn reset(&mut self) {
        self.raster_line = 0;
        self.raster_irq = 0;
    }
}

impl Default for Vic {
    fn default() -> Self {
        Self::new()
    }
}

// C64 color palette (RGBA)
pub const PALETTE: [[u8; 4]; 16] = [
    [0x00, 0x00, 0x00, 0xFF], // 0: Black
    [0xFF, 0xFF, 0xFF, 0xFF], // 1: White
    [0x88, 0x39, 0x32, 0xFF], // 2: Red
    [0x67, 0xB6, 0xBD, 0xFF], // 3: Cyan
    [0x8B, 0x3F, 0x96, 0xFF], // 4: Purple
    [0x55, 0xA0, 0x49, 0xFF], // 5: Green
    [0x40, 0x31, 0x8D, 0xFF], // 6: Blue
    [0xBF, 0xCE, 0x72, 0xFF], // 7: Yellow
    [0x8B, 0x54, 0x29, 0xFF], // 8: Orange
    [0x57, 0x42, 0x00, 0xFF], // 9: Brown
    [0xB8, 0x69, 0x62, 0xFF], // 10: Light Red
    [0x50, 0x50, 0x50, 0xFF], // 11: Dark Grey
    [0x78, 0x78, 0x78, 0xFF], // 12: Grey
    [0x94, 0xE0, 0x89, 0xFF], // 13: Light Green
    [0x78, 0x69, 0xC4, 0xFF], // 14: Light Blue
    [0x9F, 0x9F, 0x9F, 0xFF], // 15: Light Grey
];

/// Render the C64 display to an RGBA buffer.
pub fn render(memory: &Memory, buffer: &mut [u8]) {
    let ctrl1 = memory.vic_registers[0x11];
    let ctrl2 = memory.vic_registers[0x16];

    let screen_on = ctrl1 & 0x10 != 0;
    let bitmap_mode = ctrl1 & 0x20 != 0;
    let extended_bg = ctrl1 & 0x40 != 0;
    let multicolor = ctrl2 & 0x10 != 0;

    let border_color = (memory.vic_registers[0x20] & 0x0F) as usize;
    let bg_color = (memory.vic_registers[0x21] & 0x0F) as usize;

    // Fill entire buffer with border color
    let border_rgba = &PALETTE[border_color];
    for y in 0..(DISPLAY_HEIGHT as usize) {
        for x in 0..(DISPLAY_WIDTH as usize) {
            let idx = (y * DISPLAY_WIDTH as usize + x) * 4;
            buffer[idx..idx + 4].copy_from_slice(border_rgba);
        }
    }

    if !screen_on {
        return;
    }

    // Render screen content in the center
    if bitmap_mode {
        if multicolor {
            render_multicolor_bitmap(memory, buffer, bg_color);
        } else {
            render_standard_bitmap(memory, buffer);
        }
    } else if extended_bg {
        render_extended_bg_text(memory, buffer);
    } else if multicolor {
        render_multicolor_text(memory, buffer, bg_color);
    } else {
        render_standard_text(memory, buffer, bg_color);
    }
}

/// Convert screen coordinates to buffer index (accounting for border).
#[inline]
fn screen_to_buffer_idx(x: usize, y: usize) -> usize {
    ((y + BORDER_V) * DISPLAY_WIDTH as usize + (x + BORDER_H)) * 4
}

fn render_standard_text(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();

    // 40x25 character display
    for row in 0..25 {
        for col in 0..40 {
            let char_index = row * 40 + col;
            let screen_addr = screen_ptr.wrapping_add(char_index as u16);
            let char_code = memory.ram[screen_addr as usize];
            let color = (memory.color_ram[char_index] & 0x0F) as usize;

            // Get character bitmap (8 bytes per character)
            let char_addr = char_ptr.wrapping_add((char_code as u16) * 8);

            for line in 0..8 {
                let bitmap = memory.vic_read(char_addr.wrapping_add(line) & 0x3FFF);

                for bit in 0..8 {
                    let pixel_set = (bitmap >> (7 - bit)) & 1 != 0;
                    let pixel_color = if pixel_set { color } else { bg_color };

                    let x = col * 8 + bit;
                    let y = row * 8 + line as usize;
                    let idx = screen_to_buffer_idx(x, y);

                    let rgba = &PALETTE[pixel_color];
                    buffer[idx..idx + 4].copy_from_slice(rgba);
                }
            }
        }
    }
}

fn render_multicolor_text(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();

    let bg1 = (memory.vic_registers[0x22] & 0x0F) as usize;
    let bg2 = (memory.vic_registers[0x23] & 0x0F) as usize;

    for row in 0..25 {
        for col in 0..40 {
            let char_index = row * 40 + col;
            let screen_addr = screen_ptr.wrapping_add(char_index as u16);
            let char_code = memory.ram[screen_addr as usize];
            let color_byte = memory.color_ram[char_index];

            // If bit 3 of color is set, use multicolor mode for this character
            let use_multicolor = color_byte & 0x08 != 0;
            let char_color = (color_byte & 0x07) as usize;

            let char_addr = char_ptr.wrapping_add((char_code as u16) * 8);

            for line in 0..8 {
                let bitmap = memory.vic_read(char_addr.wrapping_add(line) & 0x3FFF);

                if use_multicolor {
                    // Multicolor: 2 bits per pixel, 4 pixels per byte
                    for pixel in 0..4 {
                        let bits = (bitmap >> (6 - pixel * 2)) & 0x03;
                        let pixel_color = match bits {
                            0 => bg_color,
                            1 => bg1,
                            2 => bg2,
                            3 => char_color,
                            _ => unreachable!(),
                        };

                        // Each multicolor pixel is 2 hires pixels wide
                        let x = col * 8 + pixel * 2;
                        let y = row * 8 + line as usize;

                        let rgba = &PALETTE[pixel_color];
                        for dx in 0..2 {
                            let idx = screen_to_buffer_idx(x + dx, y);
                            buffer[idx..idx + 4].copy_from_slice(rgba);
                        }
                    }
                } else {
                    // Standard text mode (but color bits 0-2 only)
                    for bit in 0..8 {
                        let pixel_set = (bitmap >> (7 - bit)) & 1 != 0;
                        let pixel_color = if pixel_set {
                            (color_byte & 0x0F) as usize
                        } else {
                            bg_color
                        };

                        let x = col * 8 + bit;
                        let y = row * 8 + line as usize;
                        let idx = screen_to_buffer_idx(x, y);

                        let rgba = &PALETTE[pixel_color];
                        buffer[idx..idx + 4].copy_from_slice(rgba);
                    }
                }
            }
        }
    }
}

fn render_extended_bg_text(memory: &Memory, buffer: &mut [u8]) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();

    let bg_colors = [
        (memory.vic_registers[0x21] & 0x0F) as usize,
        (memory.vic_registers[0x22] & 0x0F) as usize,
        (memory.vic_registers[0x23] & 0x0F) as usize,
        (memory.vic_registers[0x24] & 0x0F) as usize,
    ];

    for row in 0..25 {
        for col in 0..40 {
            let char_index = row * 40 + col;
            let screen_addr = screen_ptr.wrapping_add(char_index as u16);
            let char_byte = memory.ram[screen_addr as usize];

            // Upper 2 bits select background color
            let bg_select = (char_byte >> 6) as usize;
            let char_code = char_byte & 0x3F; // Only 64 characters available
            let fg_color = (memory.color_ram[char_index] & 0x0F) as usize;

            let char_addr = char_ptr.wrapping_add((char_code as u16) * 8);

            for line in 0..8 {
                let bitmap = memory.vic_read(char_addr.wrapping_add(line) & 0x3FFF);

                for bit in 0..8 {
                    let pixel_set = (bitmap >> (7 - bit)) & 1 != 0;
                    let pixel_color = if pixel_set {
                        fg_color
                    } else {
                        bg_colors[bg_select]
                    };

                    let x = col * 8 + bit;
                    let y = row * 8 + line as usize;
                    let idx = screen_to_buffer_idx(x, y);

                    let rgba = &PALETTE[pixel_color];
                    buffer[idx..idx + 4].copy_from_slice(rgba);
                }
            }
        }
    }
}

fn render_standard_bitmap(memory: &Memory, buffer: &mut [u8]) {
    let screen_ptr = memory.screen_ptr();

    // In bitmap mode, character pointer bits select bitmap location
    let bitmap_ptr = if memory.vic_registers[0x18] & 0x08 != 0 {
        memory.vic_bank() + 0x2000
    } else {
        memory.vic_bank()
    };

    for row in 0..25 {
        for col in 0..40 {
            let char_index = row * 40 + col;

            // Color info from screen RAM
            let screen_addr = screen_ptr.wrapping_add(char_index as u16);
            let color_byte = memory.ram[screen_addr as usize];
            let fg_color = ((color_byte >> 4) & 0x0F) as usize;
            let bg_color = (color_byte & 0x0F) as usize;

            // Bitmap data (8 bytes per cell)
            let bitmap_addr = bitmap_ptr.wrapping_add((char_index as u16) * 8);

            for line in 0..8 {
                let bitmap = memory.vic_read(bitmap_addr.wrapping_add(line) & 0x3FFF);

                for bit in 0..8 {
                    let pixel_set = (bitmap >> (7 - bit)) & 1 != 0;
                    let pixel_color = if pixel_set { fg_color } else { bg_color };

                    let x = col * 8 + bit;
                    let y = row * 8 + line as usize;
                    let idx = screen_to_buffer_idx(x, y);

                    let rgba = &PALETTE[pixel_color];
                    buffer[idx..idx + 4].copy_from_slice(rgba);
                }
            }
        }
    }
}

fn render_multicolor_bitmap(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();

    let bitmap_ptr = if memory.vic_registers[0x18] & 0x08 != 0 {
        memory.vic_bank() + 0x2000
    } else {
        memory.vic_bank()
    };

    for row in 0..25 {
        for col in 0..40 {
            let char_index = row * 40 + col;

            // Colors from screen RAM and color RAM
            let screen_addr = screen_ptr.wrapping_add(char_index as u16);
            let color_byte = memory.ram[screen_addr as usize];
            let color1 = ((color_byte >> 4) & 0x0F) as usize;
            let color2 = (color_byte & 0x0F) as usize;
            let color3 = (memory.color_ram[char_index] & 0x0F) as usize;

            let bitmap_addr = bitmap_ptr.wrapping_add((char_index as u16) * 8);

            for line in 0..8 {
                let bitmap = memory.vic_read(bitmap_addr.wrapping_add(line) & 0x3FFF);

                for pixel in 0..4 {
                    let bits = (bitmap >> (6 - pixel * 2)) & 0x03;
                    let pixel_color = match bits {
                        0 => bg_color,
                        1 => color1,
                        2 => color2,
                        3 => color3,
                        _ => unreachable!(),
                    };

                    let x = col * 8 + pixel * 2;
                    let y = row * 8 + line as usize;

                    let rgba = &PALETTE[pixel_color];
                    for dx in 0..2 {
                        let idx = screen_to_buffer_idx(x + dx, y);
                        buffer[idx..idx + 4].copy_from_slice(rgba);
                    }
                }
            }
        }
    }
}
