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

/// Cycles per raster line (PAL).
const CYCLES_PER_LINE: u32 = 63;

/// First visible raster line (display area).
const FIRST_VISIBLE_LINE: u16 = 48;

/// Last visible raster line (display area).
const LAST_VISIBLE_LINE: u16 = 247;

/// First cycle in line where badline steals (cycles 12-54).
const BADLINE_START_CYCLE: u32 = 12;

/// Last cycle in line where badline steals.
const BADLINE_END_CYCLE: u32 = 54;

/// VIC-II video chip.
pub struct Vic {
    /// Current raster line
    pub raster_line: u16,
    /// Raster interrupt line
    pub raster_irq: u16,
    /// Current cycle within the frame
    pub frame_cycle: u32,
    /// Bus Available signal (false = CPU halted due to badline)
    pub ba_low: bool,
}

impl Vic {
    pub fn new() -> Self {
        Self {
            raster_line: 0,
            raster_irq: 0,
            frame_cycle: 0,
            ba_low: false,
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

    /// Check if current line is a badline.
    ///
    /// A badline occurs when:
    /// - Display is enabled (DEN bit in $D011)
    /// - Raster line is in the visible area (48-247)
    /// - Lower 3 bits of raster line match YSCROLL
    fn is_badline(&self, vic_registers: &[u8; 64]) -> bool {
        let ctrl1 = vic_registers[0x11];
        let yscroll = ctrl1 & 0x07;
        let den = ctrl1 & 0x10 != 0; // Display enable

        if !den {
            return false;
        }

        if self.raster_line < FIRST_VISIBLE_LINE || self.raster_line > LAST_VISIBLE_LINE {
            return false;
        }

        (self.raster_line & 0x07) == yscroll as u16
    }

    /// Tick VIC for one cycle. Returns true if BA is low (CPU should halt).
    ///
    /// During badlines, VIC-II steals cycles from the CPU to fetch character
    /// data. This happens on cycles 12-54 of badlines (40 characters + setup).
    pub fn tick(&mut self, vic_registers: &[u8; 64]) -> bool {
        self.frame_cycle += 1;
        let cycle_in_line = self.frame_cycle % CYCLES_PER_LINE;
        self.raster_line = (self.frame_cycle / CYCLES_PER_LINE) as u16;

        // Badline steals cycles 12-54 on character fetch lines
        if self.is_badline(vic_registers)
            && cycle_in_line >= BADLINE_START_CYCLE
            && cycle_in_line < BADLINE_END_CYCLE
        {
            self.ba_low = true;
            return true;
        }

        self.ba_low = false;
        false
    }

    /// Reset the frame cycle counter (called at start of each frame).
    pub fn reset_frame(&mut self) {
        self.frame_cycle = 0;
        self.raster_line = 0;
        self.ba_low = false;
    }

    /// Reset the VIC-II.
    pub fn reset(&mut self) {
        self.raster_line = 0;
        self.raster_irq = 0;
        self.frame_cycle = 0;
        self.ba_low = false;
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
pub fn render(memory: &mut Memory, buffer: &mut [u8]) {
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

    // Render sprites on top of background and detect collisions
    render_sprites(memory, buffer, bg_color);
}

/// Render all enabled sprites with collision detection.
fn render_sprites(memory: &mut Memory, buffer: &mut [u8], bg_color: usize) {
    let sprite_enable = memory.vic_registers[0x15];
    if sprite_enable == 0 {
        return;
    }

    let x_expand = memory.vic_registers[0x1D];
    let y_expand = memory.vic_registers[0x17];
    let multicolor_enable = memory.vic_registers[0x1C];
    let priority = memory.vic_registers[0x1B]; // 1 = behind background
    let x_msb = memory.vic_registers[0x10];

    // Multicolor sprite colors
    let mc0 = (memory.vic_registers[0x25] & 0x0F) as usize;
    let mc1 = (memory.vic_registers[0x26] & 0x0F) as usize;

    // Sprite pointers are at screen_ptr + $3F8
    let sprite_ptr_base = memory.screen_ptr().wrapping_add(0x3F8);

    // Sprite coverage map: for each pixel, which sprite covers it (0xFF = none)
    // Used to detect sprite-sprite collisions
    let mut sprite_coverage: [[u8; 320]; 200] = [[0xFF; 320]; 200];

    // Render sprites from back to front (sprite 0 has highest priority)
    for sprite_num in (0..8).rev() {
        let mask = 1 << sprite_num;
        if sprite_enable & mask == 0 {
            continue;
        }

        // Get sprite position (9-bit X, 8-bit Y)
        let x_lo = memory.vic_registers[sprite_num * 2] as u16;
        let y = memory.vic_registers[sprite_num * 2 + 1] as i32;
        let x_hi = if x_msb & mask != 0 { 256u16 } else { 0u16 };
        let sprite_x = (x_lo | x_hi) as i32;

        // Get sprite data pointer (64 bytes per sprite block)
        let pointer = memory.ram[sprite_ptr_base.wrapping_add(sprite_num as u16) as usize];
        let data_addr = memory.vic_bank().wrapping_add((pointer as u16) * 64);

        // Sprite color
        let sprite_color = (memory.vic_registers[0x27 + sprite_num] & 0x0F) as usize;

        // Check flags for this sprite
        let is_expanded_x = x_expand & mask != 0;
        let is_expanded_y = y_expand & mask != 0;
        let is_multicolor = multicolor_enable & mask != 0;
        let is_behind_bg = priority & mask != 0;

        // Sprite is 24x21 pixels (or expanded)
        let sprite_height = if is_expanded_y { 42 } else { 21 };

        // Render sprite pixels
        for row in 0..sprite_height {
            let data_row = if is_expanded_y { row / 2 } else { row };
            let screen_y = y + row as i32;

            // Skip if outside visible area
            if screen_y < 0 || screen_y >= 200 {
                continue;
            }

            // Get 3 bytes of sprite data for this row
            let byte0 = memory.vic_read(data_addr.wrapping_add(data_row as u16 * 3)) as u32;
            let byte1 = memory.vic_read(data_addr.wrapping_add(data_row as u16 * 3 + 1)) as u32;
            let byte2 = memory.vic_read(data_addr.wrapping_add(data_row as u16 * 3 + 2)) as u32;
            let row_data = (byte0 << 16) | (byte1 << 8) | byte2;

            if is_multicolor {
                // Multicolor mode: 2 bits per pixel, 12 pixels per row
                for pixel in 0..12 {
                    let bit_pos = 22 - pixel * 2;
                    let color_bits = ((row_data >> bit_pos) & 0x03) as usize;

                    if color_bits == 0 {
                        continue; // Transparent
                    }

                    let pixel_color = match color_bits {
                        1 => mc0,
                        2 => sprite_color,
                        3 => mc1,
                        _ => unreachable!(),
                    };

                    let pixel_width = if is_expanded_x { 4 } else { 2 };
                    for dx in 0..pixel_width {
                        let screen_x = sprite_x + (pixel as i32) * pixel_width + dx;
                        draw_sprite_pixel_with_collision(
                            memory,
                            buffer,
                            &mut sprite_coverage,
                            screen_x,
                            screen_y,
                            pixel_color,
                            bg_color,
                            is_behind_bg,
                            sprite_num as u8,
                        );
                    }
                }
            } else {
                // Standard mode: 1 bit per pixel, 24 pixels per row
                for pixel in 0..24 {
                    let bit_pos = 23 - pixel;
                    if (row_data >> bit_pos) & 1 == 0 {
                        continue; // Transparent
                    }

                    let pixel_width = if is_expanded_x { 2 } else { 1 };
                    for dx in 0..pixel_width {
                        let screen_x = sprite_x + (pixel as i32) * pixel_width + dx;
                        draw_sprite_pixel_with_collision(
                            memory,
                            buffer,
                            &mut sprite_coverage,
                            screen_x,
                            screen_y,
                            sprite_color,
                            bg_color,
                            is_behind_bg,
                            sprite_num as u8,
                        );
                    }
                }
            }
        }
    }
}

/// Draw a single sprite pixel with collision detection.
fn draw_sprite_pixel_with_collision(
    memory: &mut Memory,
    buffer: &mut [u8],
    sprite_coverage: &mut [[u8; 320]; 200],
    x: i32,
    y: i32,
    color: usize,
    bg_color: usize,
    behind_bg: bool,
    sprite_num: u8,
) {
    // Sprite coordinates are relative to display, offset by 24 pixels for left border
    let display_x = x - 24;
    let display_y = y - 50;

    // Check bounds
    if display_x < 0 || display_x >= 320 || display_y < 0 || display_y >= 200 {
        return;
    }

    let dx = display_x as usize;
    let dy = display_y as usize;
    let buffer_x = dx + BORDER_H;
    let buffer_y = dy + BORDER_V;
    let idx = (buffer_y * DISPLAY_WIDTH as usize + buffer_x) * 4;

    // Check for sprite-sprite collision
    let existing_sprite = sprite_coverage[dy][dx];
    if existing_sprite != 0xFF {
        // Collision! Set bits for both sprites (latched in vic_registers)
        memory.vic_registers[0x1E] |= (1 << sprite_num) | (1 << existing_sprite);
    }

    // Mark this pixel as covered by this sprite
    sprite_coverage[dy][dx] = sprite_num;

    // Check for sprite-background collision
    let current = &buffer[idx..idx + 4];
    let bg_rgba = &PALETTE[bg_color];
    let is_background = current == bg_rgba;

    if !is_background {
        // Sprite overlaps with non-background pixel (latched in vic_registers)
        memory.vic_registers[0x1F] |= 1 << sprite_num;
    }

    // If sprite is behind background, only draw if pixel is background
    if behind_bg && !is_background {
        return;
    }

    buffer[idx..idx + 4].copy_from_slice(&PALETTE[color]);
}

/// Convert screen coordinates to buffer index (accounting for border and scroll).
#[inline]
fn screen_to_buffer_idx(x: usize, y: usize, x_scroll: usize, y_scroll: usize) -> Option<usize> {
    // Apply scroll offset
    let scrolled_x = x.wrapping_sub(x_scroll);
    let scrolled_y = y.wrapping_sub(y_scroll);

    // Check bounds (320x200 visible area)
    if scrolled_x >= 320 || scrolled_y >= 200 {
        return None;
    }

    Some(((scrolled_y + BORDER_V) * DISPLAY_WIDTH as usize + (scrolled_x + BORDER_H)) * 4)
}

/// Get scroll values from VIC registers.
fn get_scroll(memory: &Memory) -> (usize, usize, bool, bool) {
    let ctrl1 = memory.vic_registers[0x11];
    let ctrl2 = memory.vic_registers[0x16];

    let y_scroll = (ctrl1 & 0x07) as usize;
    let x_scroll = (ctrl2 & 0x07) as usize;
    let rows_25 = ctrl1 & 0x08 != 0; // true = 25 rows, false = 24 rows
    let cols_40 = ctrl2 & 0x08 != 0; // true = 40 cols, false = 38 cols

    (x_scroll, y_scroll, rows_25, cols_40)
}

fn render_standard_text(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();
    let (x_scroll, y_scroll, rows_25, cols_40) = get_scroll(memory);

    // Determine visible area based on row/column mode
    let first_col = if cols_40 { 0 } else { 1 };
    let last_col = if cols_40 { 40 } else { 39 };
    let first_row = if rows_25 { 0 } else { 1 };
    let last_row = if rows_25 { 25 } else { 24 };

    // 40x25 character display
    for row in first_row..last_row {
        for col in first_col..last_col {
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

                    if let Some(idx) = screen_to_buffer_idx(x, y, x_scroll, y_scroll) {
                        let rgba = &PALETTE[pixel_color];
                        buffer[idx..idx + 4].copy_from_slice(rgba);
                    }
                }
            }
        }
    }
}

fn render_multicolor_text(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();
    let (x_scroll, y_scroll, rows_25, cols_40) = get_scroll(memory);

    let bg1 = (memory.vic_registers[0x22] & 0x0F) as usize;
    let bg2 = (memory.vic_registers[0x23] & 0x0F) as usize;

    let first_col = if cols_40 { 0 } else { 1 };
    let last_col = if cols_40 { 40 } else { 39 };
    let first_row = if rows_25 { 0 } else { 1 };
    let last_row = if rows_25 { 25 } else { 24 };

    for row in first_row..last_row {
        for col in first_col..last_col {
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
                            if let Some(idx) = screen_to_buffer_idx(x + dx, y, x_scroll, y_scroll) {
                                buffer[idx..idx + 4].copy_from_slice(rgba);
                            }
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

                        if let Some(idx) = screen_to_buffer_idx(x, y, x_scroll, y_scroll) {
                            let rgba = &PALETTE[pixel_color];
                            buffer[idx..idx + 4].copy_from_slice(rgba);
                        }
                    }
                }
            }
        }
    }
}

fn render_extended_bg_text(memory: &Memory, buffer: &mut [u8]) {
    let screen_ptr = memory.screen_ptr();
    let char_ptr = memory.char_ptr();
    let (x_scroll, y_scroll, rows_25, cols_40) = get_scroll(memory);

    let bg_colors = [
        (memory.vic_registers[0x21] & 0x0F) as usize,
        (memory.vic_registers[0x22] & 0x0F) as usize,
        (memory.vic_registers[0x23] & 0x0F) as usize,
        (memory.vic_registers[0x24] & 0x0F) as usize,
    ];

    let first_col = if cols_40 { 0 } else { 1 };
    let last_col = if cols_40 { 40 } else { 39 };
    let first_row = if rows_25 { 0 } else { 1 };
    let last_row = if rows_25 { 25 } else { 24 };

    for row in first_row..last_row {
        for col in first_col..last_col {
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

                    if let Some(idx) = screen_to_buffer_idx(x, y, x_scroll, y_scroll) {
                        let rgba = &PALETTE[pixel_color];
                        buffer[idx..idx + 4].copy_from_slice(rgba);
                    }
                }
            }
        }
    }
}

fn render_standard_bitmap(memory: &Memory, buffer: &mut [u8]) {
    let screen_ptr = memory.screen_ptr();
    let (x_scroll, y_scroll, rows_25, cols_40) = get_scroll(memory);

    // In bitmap mode, character pointer bits select bitmap location
    let bitmap_ptr = if memory.vic_registers[0x18] & 0x08 != 0 {
        memory.vic_bank() + 0x2000
    } else {
        memory.vic_bank()
    };

    let first_col = if cols_40 { 0 } else { 1 };
    let last_col = if cols_40 { 40 } else { 39 };
    let first_row = if rows_25 { 0 } else { 1 };
    let last_row = if rows_25 { 25 } else { 24 };

    for row in first_row..last_row {
        for col in first_col..last_col {
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

                    if let Some(idx) = screen_to_buffer_idx(x, y, x_scroll, y_scroll) {
                        let rgba = &PALETTE[pixel_color];
                        buffer[idx..idx + 4].copy_from_slice(rgba);
                    }
                }
            }
        }
    }
}

fn render_multicolor_bitmap(memory: &Memory, buffer: &mut [u8], bg_color: usize) {
    let screen_ptr = memory.screen_ptr();
    let (x_scroll, y_scroll, rows_25, cols_40) = get_scroll(memory);

    let bitmap_ptr = if memory.vic_registers[0x18] & 0x08 != 0 {
        memory.vic_bank() + 0x2000
    } else {
        memory.vic_bank()
    };

    let first_col = if cols_40 { 0 } else { 1 };
    let last_col = if cols_40 { 40 } else { 39 };
    let first_row = if rows_25 { 0 } else { 1 };
    let last_row = if rows_25 { 25 } else { 24 };

    for row in first_row..last_row {
        for col in first_col..last_col {
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
                        if let Some(idx) = screen_to_buffer_idx(x + dx, y, x_scroll, y_scroll) {
                            buffer[idx..idx + 4].copy_from_slice(rgba);
                        }
                    }
                }
            }
        }
    }
}
